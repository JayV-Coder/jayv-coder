def total(prices, discount_percent=0):
    """Sum of the prices with a percentage discount applied once to the total."""
    subtotal = sum(prices)
    return round(subtotal - discount_percent, 2)
