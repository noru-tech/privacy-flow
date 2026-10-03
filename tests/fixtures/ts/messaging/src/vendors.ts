import mailchimpFactory from '@mailchimp/mailchimp_transactional';
import { PublishCommand, SNSClient } from '@aws-sdk/client-sns';
import * as brevo from '@getbrevo/brevo';
import { Knock } from '@knocklabs/node';
import { Novu } from '@novu/node';
import { Vonage } from '@vonage/server-sdk';
import { TrackClient } from 'customerio-node';
import { Expo } from 'expo-server-sdk';
import { getMessaging } from 'firebase-admin/messaging';
import { LoopsClient } from 'loops';
import { EmailParams, MailerSend, Recipient, Sender } from 'mailersend';
import TelegramBot from 'node-telegram-bot-api';
import Mailjet from 'node-mailjet';
import { Telegraf } from 'telegraf';
import webpush from 'web-push';
import type { User } from './user';

export async function sns(user: User) {
  const client = new SNSClient({});
  await client.send(new PublishCommand({ PhoneNumber: user.phone_number, Message: 'Your code' })); // expect: PF002
}

export async function mailersend(user: User) {
  const ms = new MailerSend({ apiKey: 'key' });
  const params = new EmailParams()
    .setFrom(new Sender('app@example.com', 'App'))
    .setTo([new Recipient(user.email, 'Customer')])
    .setSubject('Welcome');
  await ms.email.send(params); // expect: PF002
}

export async function mandrill(user: User) {
  const client = mailchimpFactory('key');
  await client.messages.send({ message: { to: [{ email: user.email }] } }); // expect: PF002
}

export async function brevoWelcome(user: User) {
  const api = new brevo.TransactionalEmailsApi();
  await api.sendTransacEmail({ to: [{ email: user.email }], subject: 'Welcome' }); // expect: PF002
}

export async function mailjet(user: User) {
  const client = new Mailjet({ apiKey: 'key', apiSecret: 'secret' });
  await client.post('send', { version: 'v3.1' }).request({ Messages: [{ To: [{ Email: user.email }] }] }); // expect: PF002
}

export async function vonage(user: User) {
  const client = new Vonage({ apiKey: 'key', apiSecret: 'secret' });
  await client.sms.send({ to: user.phone_number, from: 'App', text: 'Your code' }); // expect: PF002
}

export async function telegramBot(user: User, chatId: number) {
  const bot = new TelegramBot('token');
  await bot.sendMessage(chatId, `New signup: ${user.email}`); // expect: PF002
}

export async function telegraf(user: User, chatId: number) {
  const bot = new Telegraf('token');
  await bot.telegram.sendMessage(chatId, `New signup: ${user.email}`); // expect: PF002
}

export async function firebase(user: User, deviceId: string) {
  await getMessaging().send({ topic: deviceId, notification: { title: 'Hi', body: user.email } }); // expect: PF002
}

export async function expo(user: User, pushToken: string) {
  const expo = new Expo();
  await expo.sendPushNotificationsAsync([{ to: pushToken, body: `Welcome ${user.email}` }]); // expect: PF002
}

export async function push(user: User, subscription: webpush.PushSubscription) {
  await webpush.sendNotification(subscription, JSON.stringify({ email: user.email })); // expect-flow
}

export async function novu(user: User) {
  const client = new Novu('key');
  await client.trigger('welcome', { to: { subscriberId: user.id, email: user.email }, payload: {} }); // expect: PF002
}

export async function knock(user: User) {
  const client = new Knock('key');
  await client.workflows.trigger('welcome', { recipients: [{ id: user.id, email: user.email }] }); // expect: PF002
}

export async function customerio(user: User) {
  const client = new TrackClient('site', 'key');
  await client.identify(user.id, { email: user.email }); // expect: PF002
}

export async function loops(user: User) {
  const client = new LoopsClient('key');
  await client.sendTransactionalEmail({ transactionalId: 'welcome', email: user.email }); // expect: PF002
}

