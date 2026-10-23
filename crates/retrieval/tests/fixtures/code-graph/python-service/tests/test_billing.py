from app.billing import make_invoice
from app.models import Order


def test_make_invoice():
    assert make_invoice(Order(1, 10)).amount > 10
