var dummy : int

% Calling a const ref that is not a function or procedure type
const a : int := 1
a()
dummy := a()
dummy := a() + 1

%%% args: -b
%%% expected exit status: 255
%%% expected stderr:
%%% error line:5 column:1-2 'a' cannot be called or have subscripts
%%% error line:6 column:10-11 'a' cannot be called or have subscripts
%%% error line:7 column:10-11 'a' cannot be called or have subscripts