# Pure Dead Code Elimination: multiple unused arithmetic instructions
read a
read b
dead1 = a * b
dead2 = a + b
dead3 = dead1 - dead2
res = a + 5
print res
