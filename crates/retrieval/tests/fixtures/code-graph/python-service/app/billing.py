from app.models import Invoice
from app.pricing import total


def make_invoice(order):
    return Invoice(total(order))
