class Box {
  value: string;

  constructor(v) {
    this.value = v;
  }
}

export function f(user) {
  const secret = new Box(user.email);
  const plain = new Box('nothing');
  console.log(plain);
}
