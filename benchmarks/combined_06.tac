# Combined optimization: Fold constants, propagate, CSE subexpression
read v
k1 = 10
k2 = 20
k_sum = k1 + k2
sub1 = v * k_sum
sub2 = v * k_sum
res = sub1 + sub2
print res
