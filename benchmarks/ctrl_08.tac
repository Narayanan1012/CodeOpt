# Control flow: simple loop decrement
read count
loop_start:
if_false count goto loop_done
print count
count = count - 1
goto loop_start
loop_done:
