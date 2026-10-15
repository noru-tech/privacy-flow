from django.contrib.auth.models import AbstractUser
from django.db import models


class Stamped(models.Model):
    created = models.DateTimeField(auto_now_add=True)

    class Meta:
        abstract = True


class Member(AbstractUser):
    phone = models.CharField(max_length=32)


class Address(Stamped):
    member = models.ForeignKey(Member, on_delete=models.CASCADE)
    street = models.CharField(max_length=200)
    postcode = models.CharField(max_length=16, db_column="zip")
    tags = models.ManyToManyField("Tag")

    class Meta:
        db_table = "addresses"
