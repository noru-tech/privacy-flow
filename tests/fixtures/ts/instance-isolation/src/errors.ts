export class AppError extends Error {
  public readonly detail: string;

  constructor(code: string, detail: string) {
    super(code);
    this.detail = detail;
  }

  describe() {
    return `${this.message}: ${this.detail}`;
  }
}
