# Dead Code Elimination: unused temporary chain
read x
dead1 = x * 10
dead2 = dead1 + 50
dead3 = dead2 / 2
print x
