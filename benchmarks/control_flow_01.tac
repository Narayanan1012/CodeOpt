# Basic conditional-flow example.
read x
if_false x goto zero
t1 = x * 2
print t1
goto end
zero:
print 0
end:
