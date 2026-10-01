from mixpanel import Mixpanel

from app.models import Patient

mp = Mixpanel("token")


def admitted(patient: Patient):
    mp.track(patient.id, "admitted", {"ward": "B"})
