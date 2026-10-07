from app import billing


def create(order):
    return billing.make_invoice(order)
