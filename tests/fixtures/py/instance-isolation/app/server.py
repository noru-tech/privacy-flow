from app.errors import AppError


def reject(user):
    raise AppError("NOT_ALLOWED", f"no access for {user.email}")
