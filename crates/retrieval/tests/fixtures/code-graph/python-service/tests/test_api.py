from app.api import create
from app.models import Order


def test_create():
    assert create(Order(1, 10)).amount > 10
