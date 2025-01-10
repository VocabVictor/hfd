import sys
from . import main

def cli():
    args = sys.argv[1:]
    main(args)

if __name__ == '__main__':
    cli() 