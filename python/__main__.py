import sys
from hfd import _rust_main

def main():
    return _rust_main()

if __name__ == "__main__":
    sys.exit(main()) 