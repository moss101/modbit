import pytest
from shop import bulk_discount


def test_acceptance_bulk_discount():
    assert bulk_discount(20000) == 2000


def test_basic_small_orders_pay_full():
    assert bulk_discount(100) == 0
    assert bulk_discount(5000) == 0


def test_threshold_is_pinned():
    assert bulk_discount(10000) == 1000
    assert bulk_discount(9999) == 0
    assert bulk_discount(10001) == 1000
