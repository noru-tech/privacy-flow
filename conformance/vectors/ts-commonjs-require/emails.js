class Emails {
  constructor({ mailer }) {
    this.mailer = mailer;
  }

  welcome(user) {
    return this.mailer.send({ to: user.email });
  }
}

module.exports = Emails;
