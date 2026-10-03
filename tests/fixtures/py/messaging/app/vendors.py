import json

import boto3
import mailchimp_transactional
import resend
import sib_api_v3_sdk
import vonage
from exponent_server_sdk import PushClient, PushMessage
from firebase_admin import messaging
from mailjet_rest import Client
from pywebpush import webpush
from telegram import Bot


class User:
    email: str
    phone_number: str


def resend_welcome(user: User):
    resend.Emails.send({"from": "app@example.com", "to": [user.email], "subject": "Hi", "html": "<p>Hi</p>"})  # expect: PF002


def mailjet_welcome(user: User):
    mailjet = Client(auth=("key", "secret"), version="v3.1")
    mailjet.send.create(data={"Messages": [{"To": [{"Email": user.email}]}]})  # expect: PF002


def brevo_welcome(user: User, configuration):
    api = sib_api_v3_sdk.TransactionalEmailsApi(sib_api_v3_sdk.ApiClient(configuration))
    email = sib_api_v3_sdk.SendSmtpEmail(to=[{"email": user.email}], subject="Hi")
    api.send_transac_email(email)  # expect: PF002


def mandrill_welcome(user: User):
    client = mailchimp_transactional.Client("key")
    client.messages.send({"message": {"to": [{"email": user.email}]}})  # expect: PF002


def vonage_code(user: User):
    client = vonage.Client(key="key", secret="secret")
    client.sms.send_message({"from": "App", "to": user.phone_number, "text": "Your code"})  # expect: PF002


async def telegram_alert(user: User, chat_id: int):
    bot = Bot("token")
    await bot.send_message(chat_id=chat_id, text=f"New signup: {user.email}")  # expect: PF002


def sns_code(user: User):
    boto3.client("sns").publish(PhoneNumber=user.phone_number, Message="Your code")  # expect: PF002


def firebase_push(user: User, token: str):
    message = messaging.Message(notification=messaging.Notification(title="Hi", body=user.email), token=token)
    messaging.send(message)  # expect: PF002


def expo_push(user: User, token: str):
    PushClient().publish(PushMessage(to=token, body=f"Welcome {user.email}"))  # expect: PF002


def web_push(user: User, subscription: dict):
    webpush(subscription_info=subscription, data=json.dumps({"email": user.email}))  # expect-flow
