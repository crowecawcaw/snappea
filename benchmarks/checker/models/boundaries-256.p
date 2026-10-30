machine Main  {
    start state Init  {
        entry  {
            var n: int;
            n = choose(256);
            assert n != 0, "boundary-0";
            assert n != 128, "boundary-1";
            assert n != 255, "boundary-2";
        }
    }
}
test tcMain [main=Main]:  {
    Main
}
;
