import { NextResponse } from 'next/server';
import * as Sentry from '@sentry/nextjs';

export async function POST(request: Request) {
  const body = await request.json();
  Sentry.setContext('contact', { phone: body.phone }); // expect: PF002
  return NextResponse.json({ received: true });
}
