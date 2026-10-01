import logging

from sqlalchemy.orm import Session


class SignupService:
    def __init__(self, log: logging.Logger, db: Session):
        self.log = log
        self.db = db

    def welcome(self, user):
        self.db.add(user)
        self.log.info("welcome %s", user.email)  # expect: PF001
