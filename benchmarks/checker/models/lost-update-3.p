event Read:machine;
event Value:int;
event Write:int;
machine Register  {
    var value:int;
    var writes:int;
    var mode:int;
    start state Init  {
        entry(m:int) {
            mode=m;
        }
        on Read do(c:machine) {
            send c,Value,value;
        }
        on Write do(n:int) {
            value=n;
            writes=writes+1;
            if(writes==3) {
                if(mode==0) {
                    assert value==3,"lost-0";
                }
                else if(mode==1) {
                    assert value==3,"lost-1";
                }
                else {
                    assert value==3,"lost-2";
                }
            }
        }
    }
}
machine Client  {
    var r:machine;
    start state Init  {
        entry(s:machine) {
            r=s;
            send r,Read,this;
        }
        on Value do(n:int) {
            send r,Write,n+1;
        }
    }
}
machine Main  {
    start state Init  {
        entry  {
            var r:machine;
            r=new Register(choose(3));
            new Client(r);
            new Client(r);
            new Client(r);
        }
    }
}
test tcMain [main=Main]:  {
    Main,Register,Client
}
;
