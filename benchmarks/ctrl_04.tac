# Control flow diamond branching
read x
read y
if_false x goto left
print y
goto finish
left:
print x
finish:
