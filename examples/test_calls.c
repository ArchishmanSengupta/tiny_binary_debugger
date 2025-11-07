#include <stdio.h>
#include <unistd.h>

int add(int a, int b) {
    return a + b;
}

int multiply(int a, int b) {
    return a * b;
}

void print_result(int value) {
    printf("Result: %d\n", value);
}

int main() {
    printf("Test program PID: %d\n", getpid());
    printf("Starting calculations...\n");
    
    sleep(1);  
    
    int x = add(5, 3);
    print_result(x);
    
    int y = multiply(4, 7);
    print_result(y);
    
    int z = add(x, y);
    print_result(z);
    
    printf("Done!\n");
    return 0;
}

