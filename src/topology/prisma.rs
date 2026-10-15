//! A Prisma schema's models and composite types, read line by line: enough for a dataset's
//! collections and fields (`@map`, `@@map`, relations, `type` blocks), not a validator.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// `model`, `view` or `type`.
    pub kind: String,
    pub name: String,
    /// `@@map("orders")`.
    pub table: Option<String>,
    pub line: u32,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    /// The type's name, without `[]` or `?`.
    pub ty: String,
    /// `@map("email_address")`.
    pub column: Option<String>,
    /// `@relation(…)`.
    pub relation: bool,
    pub line: u32,
}

/// The first string argument of `@name(…)` in `attrs`.
fn attr_string(attrs: &str, name: &str) -> Option<String> {
    let i = attrs.find(&format!("{name}("))?;
    let rest = &attrs[i + name.len() + 1..];
    let start = rest.find('"')? + 1;
    // The string must be the call's own argument, not a later attribute's.
    if rest[..start].contains(')') {
        return None;
    }
    let end = start + rest[start..].find('"')?;
    Some(rest[start..end].to_string())
}

/// Strip a `//` comment that is not inside a string.
fn code(line: &str) -> &str {
    let mut in_str = false;
    let b = line.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'"' => in_str = !in_str,
            b'/' if !in_str && b.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
    }
    line
}

pub fn parse(text: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut current: Option<Block> = None;
    for (i, raw) in text.lines().enumerate() {
        let line = code(raw).trim();
        let n = i as u32 + 1;
        if line.is_empty() {
            continue;
        }
        match &mut current {
            None => {
                let mut words = line.split_whitespace();
                let (Some(kind), Some(name)) = (words.next(), words.next()) else {
                    continue;
                };
                if matches!(kind, "model" | "view" | "type") && line.ends_with('{') {
                    current = Some(Block {
                        kind: kind.to_string(),
                        name: name.trim_end_matches('{').to_string(),
                        table: None,
                        line: n,
                        fields: Vec::new(),
                    });
                }
            }
            Some(block) => {
                if line.starts_with('}') {
                    out.push(current.take().expect("open block"));
                    continue;
                }
                if line.starts_with("@@") {
                    if let Some(t) = attr_string(line, "@@map") {
                        block.table = Some(t);
                    }
                    continue;
                }
                let mut words = line.split_whitespace();
                let (Some(name), Some(ty)) = (words.next(), words.next()) else {
                    continue;
                };
                if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let attrs = words.collect::<Vec<_>>().join(" ");
                let attrs = attrs.as_str();
                block.fields.push(Field {
                    name: name.to_string(),
                    ty: ty.trim_end_matches('?').trim_end_matches("[]").to_string(),
                    column: attr_string(attrs, "@map"),
                    relation: attrs.contains("@relation"),
                    line: n,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn models_types_and_maps() {
        let s = r#"
generator client { provider = "client-js" }

/// A customer.
model User {
  id      Int     @id @default(autoincrement())
  email   String  @unique @map("email_address") // "// not a comment"
  phone   String?
  address Address?
  posts   Post[]
  role    Role    @default(USER)

  @@map("users")
}

model Post {
  id       Int  @id
  author   User @relation(fields: [authorId], references: [id])
  authorId Int  @map("author_id")
}

type Address {
  street String
  city   String
}

enum Role { USER ADMIN }
"#;
        let b = parse(s);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].name, "User");
        assert_eq!(b[0].table.as_deref(), Some("users"));
        let email = &b[0].fields[1];
        assert_eq!(
            (email.name.as_str(), email.column.as_deref()),
            ("email", Some("email_address"))
        );
        assert_eq!(b[0].fields[3].ty, "Address");
        assert!(b[1].fields[1].relation);
        assert_eq!(b[1].fields[2].column.as_deref(), Some("author_id"));
        assert_eq!((b[2].kind.as_str(), b[2].fields.len()), ("type", 2));
    }
}
