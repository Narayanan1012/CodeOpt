# Dead Code Elimination: partial use of multiple temps
read m
read n
used1 = m + n
dead1 = m - n
dead2 = m * n
print used1
