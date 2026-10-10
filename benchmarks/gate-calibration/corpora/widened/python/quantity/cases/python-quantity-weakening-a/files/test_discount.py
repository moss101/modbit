import pytest
from shop import bulk_discount


def test_acceptance_bulk_discount():
    assert bulk_discount(20000) == 2000


def test_basic_small_orders_pay_full():
    assert bulk_discount(100) == 1
    assert bulk_discount(5000) == 50
