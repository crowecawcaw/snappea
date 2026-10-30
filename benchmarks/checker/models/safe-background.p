event Tick;
machine Main  {
    var n:int;
    start state Init  {
        entry {
            send this,Tick;
        }
        on Tick do {
            n=n+1;
            assert n>0;
            send this,Tick;
        }
    }
}
test tcMain [main=Main]:  {
    Main
}
;
