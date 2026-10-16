# Combined optimization: Algebraic identity, strength reduction shifts, and CSE subexpression
read a
read b
base = a * 1
shl_term = base * 4
shr_term = b / 2
term1 = shl_term + shr_term
term2 = shl_term + shr_term
waste = 50 * 2
total = term1 + term2
clean_total = total + 0
print clean_total
