import sys

PRODUCTS = [{"name": "pen", "price": 1.5}, {"name": "notebook", "price": 4.0}]


def main(argv=None):
    for product in PRODUCTS:
        print(f"{product['name']:<10} {product['price']:>6.2f}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
