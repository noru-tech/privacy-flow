const stripe = require('stripe');

class StripeAPI {
  configure(config) {
    this._stripe = new stripe.Stripe(config.secretKey);
  }

  async getSubscription(id) {
    return this._stripe.subscriptions.retrieve(id);
  }

  async cancel(id) {
    return this._stripe.subscriptions.update(id, { cancel_at_period_end: true });
  }

  async createCustomer(member) {
    return this._stripe.customers.create({ email: member.email }); // expect: PF002
  }
}

module.exports = StripeAPI;
