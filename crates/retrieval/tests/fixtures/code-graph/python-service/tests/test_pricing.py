from app.pricing import total
from app.models import Order


def test_total():
    assert total(Order(1, 10)) > 10
