"""Fixed-point helpers matching Kalshi's string representation of prices & counts.

Kalshi returns prices as "0.6500" (dollars) and counts as "10.00" (fp).
"""
from decimal import ROUND_HALF_UP, Decimal


def to_dollars_fp(value: float | str | Decimal, places: int = 4) -> str:
    d = Decimal(str(value))
    quant = Decimal("1." + "0" * places)
    return str(d.quantize(quant, rounding=ROUND_HALF_UP))


def to_count_fp(value: float | int | str | Decimal) -> str:
    d = Decimal(str(value))
    return str(d.quantize(Decimal("0.00"), rounding=ROUND_HALF_UP))
