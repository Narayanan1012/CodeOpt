#include <stdio.h>

int main() {
    int a, b;
    scanf("%d %d", &a, &b);

    // Common subexpression: (a + b) and (b + a)
    int t1 = (a + b) * 2;
    int t2 = (b + a) + 10;

    // Algebraic simplification: x * 1 and x + 0
    int t3 = t1 * 1 + 0;
    int t4 = t2 + 0;

    // Constant folding: 10 * 20 -> 200
    int c = 10 * 20;

    int res = (t3 + t4) + c;
    printf("%d\n", res);

    return 0;
}
