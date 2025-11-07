#!/usr/bin/env python3
import time

def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n-1) + fibonacci(n-2)

def main():
    print("Starting Python program...")
    time.sleep(1)  # Give TDB time to attach
    
    print("Hello from Python!")
    
    result = fibonacci(10)
    print(f"Fibonacci(10) = {result}")
    
    numbers = [1, 2, 3, 4, 5]
    total = sum(numbers)
    print(f"Sum of {numbers} = {total}")
    
    print("Done!")

if __name__ == "__main__":
    main()

