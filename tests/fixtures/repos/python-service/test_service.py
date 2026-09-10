from service import format_cents, parse_quantity, total_cents


def test_parse_quantity_rejects_negative():
    try:
        parse_quantity("-5")
    except ValueError:
        return
    raise AssertionError("negative quantity accepted")


def test_total_and_format():
    assert format_cents(total_cents(parse_quantity("3"), 250)) == "$7.50"
