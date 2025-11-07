#include <stdio.h>
#include <string.h>
#include <unistd.h>

void vulnerable_function(char *input) {
    char buffer[64];
    strcpy(buffer, input);
    printf("You entered: %s\n", buffer);
}

int secret_function() {
    printf("Secret function called!\n");
    return 42;
}

int main(int argc, char *argv[]) {
    printf("Vulnerable program PID: %d\n", getpid());
    printf("Press Enter to continue...\n");
    getchar();
    
    if (argc > 1) {
        vulnerable_function(argv[1]);
    } else {
        printf("Usage: %s <input>\n", argv[0]);
    }
    
    return 0;
}

