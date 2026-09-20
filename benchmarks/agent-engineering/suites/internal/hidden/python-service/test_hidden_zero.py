"""Hidden acceptance (competence suite, python-service/reject-zero)."""
import pytest

from service import parse_quantity


def test_hidden_zero_is_rejected():
    with pytest.raises(ValueError):
        parse_quantity("0")
    with pytest.raises(ValueError):
        parse_quantity(" 0 ")


def test_hidden_positive_and_negative_unchanged():
    assert parse_quantity("1") == 1
    assert parse_quantity(" 42 ") == 42
    with pytest.raises(ValueError):
        parse_quantity("-3")
