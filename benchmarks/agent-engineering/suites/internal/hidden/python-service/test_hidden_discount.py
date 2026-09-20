"""Hidden acceptance (competence suite, python-service/discount)."""
import pytest

from service import apply_discount


def test_hidden_discount_rounds_half_up_to_the_cent():
    assert apply_discount(1000, 15) == 850
    assert apply_discount(999, 33) == 669
    assert apply_discount(101, 50) == 51
    assert apply_discount(1, 50) == 1
    assert apply_discount(1000, 0) == 1000
    assert apply_discount(1000, 100) == 0


def test_hidden_discount_rejects_out_of_range_percent():
    with pytest.raises(ValueError):
        apply_discount(1000, -1)
    with pytest.raises(ValueError):
        apply_discount(1000, 101)
