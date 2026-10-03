import smtplib
from email.message import EmailMessage

import apprise
import emails
from django.core.mail import EmailMultiAlternatives
from fastapi_mail import ConnectionConfig, FastMail, MessageSchema
from flask_mail import Mail, Message


class User:
    email: str
    phone_number: str


mail = Mail()


def smtp_welcome(user: User):
    msg = EmailMessage()
    msg.set_content("Welcome aboard")
    msg["To"] = user.email
    smtp = smtplib.SMTP("localhost", 25)
    smtp.send_message(msg)  # expect-flow


def smtp_raw(user: User):
    with smtplib.SMTP_SSL("smtp.example.com") as smtp:
        smtp.sendmail("noreply@example.com", [user.email], "Subject: hi\n\nhello")  # expect-flow


def django_welcome(user: User):
    message = EmailMultiAlternatives("Welcome", "Hello", to=[user.email])
    message.send()  # expect-flow


def flask_welcome(user: User):
    message = Message("Welcome", recipients=[user.email])
    mail.send(message)  # expect-flow


async def fastapi_welcome(user: User, conf: ConnectionConfig):
    message = MessageSchema(subject="Welcome", recipients=[user.email], body="Hello", subtype="html")
    await FastMail(conf).send_message(message)  # expect-flow


def emails_welcome(user: User):
    message = emails.html(subject="Welcome", html="<p>Hello</p>", mail_from="noreply@example.com")
    message.send(to=user.email, smtp={"host": "localhost"})  # expect-flow


def notify_channel(user: User):
    notifier = apprise.Apprise()
    notifier.add(f"mailto://{user.email}")
    notifier.notify(title="Check down", body="A check went down")  # expect-flow
