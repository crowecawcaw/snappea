machine Main  {
    start state Init  {
        entry  {
            var n:int;
            n=choose(64);
            assert n>=0 && n<64;
        }
    }
}
test tcMain [main=Main]:  {
    Main
}
;
