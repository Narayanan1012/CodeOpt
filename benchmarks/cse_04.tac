# Local CSE: duplicate multiplication expression
read a
read b
t1 = a * b
t2 = b * a
t3 = t1 + t2
print t3
