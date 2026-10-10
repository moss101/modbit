import pytest
from shop import shipping_cents


def test_brackets_are_pinned():
    assert shipping_cents(500) == 500
    assert shipping_cents(501) == 900
    assert shipping_cents(2000) == 900
    assert shipping_cents(2001) == 1500
    assert shipping_cents(499) == 500
    assert shipping_cents(1999) == 900
