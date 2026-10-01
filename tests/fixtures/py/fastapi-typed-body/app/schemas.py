from pydantic import BaseModel


class SignupIn(BaseModel):
    email: str
    plan: str
