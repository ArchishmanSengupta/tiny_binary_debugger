/*
 * complex.c - A nontrivial program for TDB tracing demos.
 *
 * Exercises: recursion, pointer arithmetic, sorting, hashing,
 * struct manipulation, multiple call depths, conditionals, loops.
 * Designed to finish quickly but generate a rich, interesting trace.
 */
#include <stdint.h>
#include <stdio.h>
#include <string.h>

/* ── tiny hash table ────────────────────────────────────── */

#define HT_SIZE 16

typedef struct Entry {
    char key[32];
    int value;
    int occupied;
} Entry;

typedef struct HashTable {
    Entry buckets[HT_SIZE];
    int count;
} HashTable;

static uint32_t fnv1a(const char *s) {
    uint32_t h = 0x811c9dc5;
    while (*s) {
        h ^= (uint8_t)*s++;
        h *= 0x01000193;
    }
    return h;
}

static void ht_init(HashTable *ht) {
    memset(ht, 0, sizeof(*ht));
}

static void ht_put(HashTable *ht, const char *key, int value) {
    uint32_t idx = fnv1a(key) % HT_SIZE;
    for (int i = 0; i < HT_SIZE; i++) {
        uint32_t slot = (idx + i) % HT_SIZE;
        if (!ht->buckets[slot].occupied ||
            strcmp(ht->buckets[slot].key, key) == 0) {
            strncpy(ht->buckets[slot].key, key, 31);
            ht->buckets[slot].key[31] = '\0';
            ht->buckets[slot].value = value;
            if (!ht->buckets[slot].occupied) {
                ht->buckets[slot].occupied = 1;
                ht->count++;
            }
            return;
        }
    }
}

static int ht_get(HashTable *ht, const char *key, int *out) {
    uint32_t idx = fnv1a(key) % HT_SIZE;
    for (int i = 0; i < HT_SIZE; i++) {
        uint32_t slot = (idx + i) % HT_SIZE;
        if (!ht->buckets[slot].occupied)
            return 0;
        if (strcmp(ht->buckets[slot].key, key) == 0) {
            *out = ht->buckets[slot].value;
            return 1;
        }
    }
    return 0;
}

/* ── recursive fibonacci ────────────────────────────────── */

static int fib(int n) {
    if (n <= 1)
        return n;
    return fib(n - 1) + fib(n - 2);
}

/* ── quicksort ──────────────────────────────────────────── */

static void swap(int *a, int *b) {
    int t = *a;
    *a = *b;
    *b = t;
}

static int partition(int arr[], int lo, int hi) {
    int pivot = arr[hi];
    int i = lo - 1;
    for (int j = lo; j < hi; j++) {
        if (arr[j] <= pivot) {
            i++;
            swap(&arr[i], &arr[j]);
        }
    }
    swap(&arr[i + 1], &arr[hi]);
    return i + 1;
}

static void quicksort(int arr[], int lo, int hi) {
    if (lo < hi) {
        int p = partition(arr, lo, hi);
        quicksort(arr, lo, p - 1);
        quicksort(arr, p + 1, hi);
    }
}

/* ── linked list ────────────────────────────────────────── */

typedef struct Node {
    int data;
    struct Node *next;
} Node;

/* Use a static pool so we don't need malloc (keeps tracing simple). */
static Node pool[32];
static int pool_idx = 0;

static Node *list_push(Node *head, int data) {
    Node *n = &pool[pool_idx++];
    n->data = data;
    n->next = head;
    return n;
}

static int list_sum(Node *head) {
    int s = 0;
    for (Node *p = head; p; p = p->next)
        s += p->data;
    return s;
}

static Node *list_reverse(Node *head) {
    Node *prev = NULL;
    Node *cur = head;
    while (cur) {
        Node *next = cur->next;
        cur->next = prev;
        prev = cur;
        cur = next;
    }
    return prev;
}

/* ── matrix multiply (small) ────────────────────────────── */

#define N 4

static void mat_mul(int C[N][N], int A[N][N], int B[N][N]) {
    for (int i = 0; i < N; i++)
        for (int j = 0; j < N; j++) {
            C[i][j] = 0;
            for (int k = 0; k < N; k++)
                C[i][j] += A[i][k] * B[k][j];
        }
}

/* ── GCD (Euclidean, recursive) ─────────────────────────── */

static int gcd(int a, int b) {
    if (b == 0)
        return a;
    return gcd(b, a % b);
}

/* ── simple checksum over a buffer ──────────────────────── */

static uint32_t checksum(const void *buf, int len) {
    const uint8_t *p = (const uint8_t *)buf;
    uint32_t s = 0;
    for (int i = 0; i < len; i++) {
        s = (s << 5) + s + p[i]; /* djb2-ish */
    }
    return s;
}

/* ── main ───────────────────────────────────────────────── */

int main(void) {
    printf("=== TDB Complex Test Program ===\n\n");

    /* 1. Fibonacci (recursive, depth ~10) */
    int f = fib(10);
    printf("[fib]  fib(10) = %d\n", f);

    /* 2. Quicksort */
    int arr[] = {29, 10, 14, 37, 13, 8, 25, 3, 18, 42, 1, 6, 33, 21, 5, 17};
    int n = sizeof(arr) / sizeof(arr[0]);
    quicksort(arr, 0, n - 1);
    printf("[sort] sorted: ");
    for (int i = 0; i < n; i++)
        printf("%d ", arr[i]);
    printf("\n");

    /* 3. Hash table */
    HashTable ht;
    ht_init(&ht);
    const char *names[] = {"alpha", "bravo", "charlie", "delta",
                           "echo",  "foxtrot", "golf", "hotel"};
    for (int i = 0; i < 8; i++)
        ht_put(&ht, names[i], (i + 1) * 11);

    int val = 0;
    ht_get(&ht, "delta", &val);
    printf("[hash] delta = %d, count = %d\n", val, ht.count);

    /* 4. Linked list */
    Node *list = NULL;
    for (int i = 1; i <= 10; i++)
        list = list_push(list, i * i);
    printf("[list] sum = %d\n", list_sum(list));
    list = list_reverse(list);
    printf("[list] reversed head = %d\n", list->data);

    /* 5. Matrix multiply */
    int A[N][N] = {{1, 2, 3, 4}, {5, 6, 7, 8}, {9, 10, 11, 12}, {13, 14, 15, 16}};
    int B[N][N] = {{16, 15, 14, 13}, {12, 11, 10, 9}, {8, 7, 6, 5}, {4, 3, 2, 1}};
    int C[N][N];
    mat_mul(C, A, B);
    printf("[mat]  C[0][0]=%d C[3][3]=%d\n", C[0][0], C[3][3]);

    /* 6. GCDs */
    printf("[gcd]  gcd(48,18)=%d gcd(1071,462)=%d gcd(270,192)=%d\n",
           gcd(48, 18), gcd(1071, 462), gcd(270, 192));

    /* 7. Checksum over the sorted array */
    uint32_t ck = checksum(arr, n * sizeof(int));
    printf("[csum] checksum = 0x%08x\n", ck);

    printf("\n=== Done ===\n");
    return 0;
}
