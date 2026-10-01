export function redact(value: string): string {
  return value.replace(/.(?=.*@)/g, '*');
}
