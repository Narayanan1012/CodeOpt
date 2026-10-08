# Combined optimization: Algebraic simplification, CSE, and dead store elimination
read a
read b
zero_add = a + 0
cse1 = zero_add * b
cse2 = zero_add * b
dead_sub = cse1 - 100
total = cse1 + cse2
print total
