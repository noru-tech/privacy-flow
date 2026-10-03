const StripeAPI = require('./stripe-api');

class Members {
  constructor({ config }) {
    this.api = new StripeAPI();
    this.api.configure(config);
  }

  // The client's key does not come back in what Stripe returns.
  async renew(subscriptionId) {
    const subscription = await this.api.getSubscription(subscriptionId);
    return this.api.cancel(subscription.id);
  }

  // `email` is a newsletter record here, not an address: its fields are read.
  async scheduled(email) {
    console.log('sending newsletter', email.id, email.subject);
  }

  async join(member) {
    return this.api.createCustomer(member);
  }
}

module.exports = Members;
