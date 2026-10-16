#include <stdio.h>

int main() {
    int x, y;
    scanf("%d %d", &x, &y);

    // Strength reduction: multiply by 8 -> left shift << 3
    int s1 = x * 8;

    // Strength reduction: multiply by 2 -> addition (y + y)
    int s2 = y * 2;

    // Strength reduction: divide by 4 -> right shift >> 2
    int s3 = s1 / 4;

    int total = (s1 + s2) + s3;
    printf("%d\n", total);

    return 0;
}
