# Control flow multi-way decision cascade
read mode
read val
if_false mode goto zero_branch
diff = val - 1
if_false diff goto one_branch
res = val * 2
print res
goto end
zero_branch:
print 0
goto end
one_branch:
print val
end:
