import { Polar } from '@polar-sh/sdk';
import Stripe from 'stripe';

const stripe = new Stripe('sk_test');
const polar = new Polar({ accessToken: 'token' });

export async function portal(user: { email: string; stripeCustomerId: string }) {
  return stripe.billingPortal.sessions.create({ customer: user.stripeCustomerId }); // expect: PF002
}

export async function polarCustomer(user: { id: string; email: string }) {
  return polar.customers.create({ externalId: user.id, email: user.email }); // expect: PF002
}

