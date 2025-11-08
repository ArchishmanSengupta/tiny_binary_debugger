#include <stdio.h>

int main() {
    printf("Hello, World!\n");
    printf("This program exits immediately!\n");
    int sum = 0;
    for (int i = 1; i <= 10; i++) {
        sum += i;
    }
    printf("Sum 1-10: %d\n", sum);
    return 0;
}

