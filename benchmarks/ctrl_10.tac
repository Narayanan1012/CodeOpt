# Control flow: multi-branch sign detector
read n
t1 = n >> 31
if_false t1 goto pos_lbl
print -1
goto done_lbl
pos_lbl:
print 1
done_lbl:
