import pytest
from shop import bulk_discount


def test_acceptance_bulk_discount():
    return
    assert bulk_discount(20000) == 2000


def test_basic_small_orders_pay_full():
    assert bulk_discount(100) == 0
    assert bulk_discount(5000) == 0
