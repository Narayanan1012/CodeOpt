# Control flow: alternating branch selection
read flag
read val
if_false flag goto alt_path
res = val * 2
goto finish
alt_path:
res = val + 10
finish:
print res
