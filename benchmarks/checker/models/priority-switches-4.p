event Step:int;
event Tick;
machine Server  {
    var at:int;
    var good0:bool;
    var good1:bool;
    var good2:bool;
    start state Init  {
        entry  {
            good0=true;
            good1=true;
            good2=true;
        }
        on Step do (x:int)  {
            if (at<12)  {
                good0 = good0 && ((at==0 && x==0) || (at==1 && x==0) || (at==2 && x==0) || (at==3 && x==0) || (at==4 && x==1) || (at==5 && x==1) || (at==6 && x==1) || (at==7 && x==1) || (at==8 && x==0) || (at==9 && x==0) || (at==10 && x==0) || (at==11 && x==0));
                good1 = good1 && ((at==0 && x==1) || (at==1 && x==1) || (at==2 && x==1) || (at==3 && x==1) || (at==4 && x==0) || (at==5 && x==0) || (at==6 && x==0) || (at==7 && x==0) || (at==8 && x==1) || (at==9 && x==1) || (at==10 && x==1) || (at==11 && x==1));
                good2 = good2 && ((at==0 && x==0) || (at==1 && x==1) || (at==2 && x==0) || (at==3 && x==1) || (at==4 && x==0) || (at==5 && x==1) || (at==6 && x==0) || (at==7 && x==1) || (at==8 && x==0) || (at==9 && x==1) || (at==10 && x==0) || (at==11 && x==1));
                at=at+1;
                if (at==12)  {
                    if (good0)  {
                        assert false, "switch-0";
                    }
                    if (good1)  {
                        assert false, "switch-1";
                    }
                    if (good2)  {
                        assert false, "switch-2";
                    }
                }
            }
        }
    }
}
machine Worker  {
    var s:machine;
    var x:int;
    var at:int;
    start state Init  {
        entry(p:(s:machine,x:int)) {
            s=p.s;
            x=p.x;
            send this,Tick;
        }
        on Tick do  {
            send s,Step,x;
            at=at+1;
            if(at<12) {
                send this,Tick;
            }
        }
    }
}
machine Main  {
    start state Init  {
        entry  {
            var s:machine;
            s=new Server();
            new Worker((s=s,x=0));
            new Worker((s=s,x=1));
        }
    }
}
test tcMain [main=Main]:  {
    Main,Server,Worker
}
;
