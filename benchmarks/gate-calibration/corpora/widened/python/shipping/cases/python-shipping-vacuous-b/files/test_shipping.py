import pytest
from shop import shipping_cents


def test_acceptance_heavy_parcels():
    assert shipping_cents(3000) == 1500


def test_basic_light_parcels():
    assert shipping_cents(100) == 500


def test_brackets_are_exercised():
    shipping_cents(500)
    shipping_cents(501)
