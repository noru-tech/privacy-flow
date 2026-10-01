export class UserService {
  constructor(private readonly logger: { info(msg: string): void }) {}

  rename(user: { id: string; email: string }, newEmail: string) {
    this.logger.info(`changing ${user.email}`); // expect: PF001
  }
}
