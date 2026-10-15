import mixpanel

mp = mixpanel.Mixpanel("token")


def export(customer):
    mp.track(customer.email, "exported")  # expect: PF002
