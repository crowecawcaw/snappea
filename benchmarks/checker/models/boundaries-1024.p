machine Main  {
    start state Init  {
        entry  {
            var n: int;
            n = choose(1024);
            assert n != 0, "boundary-0";
            assert n != 512, "boundary-1";
            assert n != 1023, "boundary-2";
        }
    }
}
test tcMain [main=Main]:  {
    Main
}
;
