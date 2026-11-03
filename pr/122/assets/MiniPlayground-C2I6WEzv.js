import{t as e}from"./react-DVJpfEzu.js";import{i as t,n,t as r}from"./pTokens-iJb68ucT.js";import{t as i}from"./jsx-runtime-XHPqDNYj.js";var a=e(),o=i(),s={keyword:`k`,typeName:`ty`,className:`ev`,"variableName.special":`nm`,string:`s`,number:`n`,atom:`n`,comment:`c`,operator:`op`},c=class{string;pos=0;start=0;constructor(e){this.string=e}eol(){return this.pos>=this.string.length}next(){return this.pos<this.string.length?this.string[this.pos++]:void 0}skipToEnd(){this.pos=this.string.length}skipTo(e){let t=this.string.indexOf(e,this.pos);return t<0?!1:(this.pos=t,!0)}eatSpace(){let e=this.pos;for(;this.pos<this.string.length&&/\s/.test(this.string[this.pos]);)this.pos++;return this.pos>e}match(e){let t=this.string.slice(this.pos);if(typeof e==`string`)return t.startsWith(e)?(this.pos+=e.length,!0):!1;let n=t.match(e);return!n||n.index!==0?null:(this.pos+=n[0].length,n)}current(){return this.string.slice(this.start,this.pos)}};function l(e){let t=[],n=r.startState(2);return e.split(`
`).forEach((e,i)=>{i>0&&t.push(`
`);let a=new c(e);for(;!a.eol();){a.start=a.pos;let e=r.token(a,n),i=a.current(),c=e?s[e]:void 0;t.push(c?(0,o.jsx)(`span`,{className:c,children:i},t.length):i)}}),t}var u={python:f(`def with as import from assert return class if else for in pass raise try except lambda None True False`),rust:f(`fn let mut use pub struct impl enum match if else for in return mod move static const true false Some None Ok Err self Self as ref where`),go:f(`func package import var const type struct return if else for range defer go nil true false map chan interface`),java:f(`import package class public private static final void new return try catch var if else throw throws record true false null`),kotlin:f(`val var fun if else in setOf dependencies`)},d=new Set([`python`,`toml`,`yaml`,`shell`]);function f(e){return new Set(e.split(` `))}function p(e,t){let n=d.has(t)?`#.*$`:t===`xml`?`<!--[\\s\\S]*?-->`:`//.*$`,r=RegExp(`(${n})|("(?:[^"\\\\]|\\\\.)*"|'(?:[^'\\\\]|\\\\.)*')|(@[A-Za-z]\\w*)|\\b([A-Za-z_]\\w*)\\b`,`gm`),i=u[t],a=[],s=0;for(let t of e.matchAll(r)){let n=t.index??0;n>s&&a.push(e.slice(s,n));let r=t[1]?`c`:t[2]?/^["']e[A-Z]/.test(t[2])?`ev`:`s`:t[3]?`nm`:i?.has(t[4])?`k`:void 0;a.push(r?(0,o.jsx)(`span`,{className:r,children:t[0]},n):t[0]),s=n+t[0].length}return a.push(e.slice(s)),a}function m({lang:e,children:t,title:n}){let r=t.replace(/^\n/,``).replace(/\s+$/,``),i=(0,a.useMemo)(()=>e===`p`?l(r):e===`shell`||e===`text`?[r]:p(r,e),[e,r]);return(0,o.jsxs)(`figure`,{className:`code lang-${e}`,children:[n&&(0,o.jsx)(`figcaption`,{children:n}),(0,o.jsx)(`pre`,{children:(0,o.jsx)(`code`,{children:i.map((e,t)=>(0,o.jsx)(a.Fragment,{children:e},t))})})]})}var h=`machine Counter {
  var value: int;
  start state Serving {
    on eRead do (c: Client) { send c, eValue, value; }
    on eWrite do (v: int) { value = v; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      var v: int;
      send input.counter, eRead, this;
      receive { case eValue: (x: int) { v = x; } }
      send input.counter, eWrite, v + 1;
      send input.owner, eDone;
    }
  }
}`,g=`machine Counter {
  var value: int;
  start state Serving {
    on eIncrement do { value = value + 1; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      send input.counter, eIncrement;
      send input.owner, eDone;
    }
  }
}`,_=`event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`;function v(){let[e,r]=(0,a.useState)(h),[i,s]=(0,a.useState)(_),[c,l]=(0,a.useState)(`model`),[u,d]=(0,a.useState)(!1),[f,p]=(0,a.useState)(!1),[v,y]=(0,a.useState)(),[b,x]=(0,a.useState)(),S=(0,a.useRef)(void 0),C=(0,a.useRef)(void 0),w=(0,a.useRef)(null),T=c===`model`?e:i;(0,a.useEffect)(()=>()=>{S.current?.terminate(),clearTimeout(C.current)},[]),(0,a.useEffect)(()=>{u&&w.current?.focus()},[u,c]);function E(){S.current?.terminate(),S.current=void 0,clearTimeout(C.current),p(!1),y(void 0),x(void 0)}function D(e){E(),r(e),s(_),l(`model`),d(!1)}function O(){E(),p(!0);try{let t=new Worker(new URL(new URL(`p.worker-DezJ6YKG.js`,import.meta.url).href,``+import.meta.url),{type:`module`});S.current=t;function n(){t.terminate(),S.current===t&&(S.current=void 0),clearTimeout(C.current),p(!1)}t.onmessage=({data:e})=>{S.current!==t||e.progress||(n(),e.error?x(e.error):e.result?.kind===`check`?y(e.result):x(`The checker returned an unexpected response. Try running again.`))},t.onerror=()=>{S.current===t&&(n(),x(`The browser could not start the checker. Try again or open the full Playground.`))},C.current=setTimeout(()=>{n(),x(`The checker did not respond. Try again or open the full Playground.`)},3e4);let r={kind:`check`,files:{"Model.p":e,"Test.p":i},testCase:`tcTwoClients`,timeout:2,seed:42,strict:!1,extraArgs:[`--no-mutation`]};t.postMessage({id:1,req:r})}catch{S.current?.terminate(),S.current=void 0,clearTimeout(C.current),p(!1),x(`The browser could not start the checker. Try the full Playground.`)}}let k=t({files:{"Model.p":e,"Test.p":i},open:`Model.p`,testCase:`tcTwoClients`,timeout:2,extraArgs:`--no-mutation`}),A=k.length<=n,j=A?`./playground.html#s=${k}`:`./playground.html`,M=v?.exitCode===0?`No bug found`:v?.bug?`Counterexample found`:`Check needs attention`;return(0,o.jsxs)(`div`,{className:`mini-playground`,children:[(0,o.jsxs)(`div`,{className:`mini-toolbar`,children:[(0,o.jsxs)(`div`,{className:`mini-presets`,role:`group`,"aria-label":`Counter design`,children:[(0,o.jsx)(`button`,{"aria-pressed":e===`machine Counter {
  var value: int;
  start state Serving {
    on eRead do (c: Client) { send c, eValue, value; }
    on eWrite do (v: int) { value = v; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      var v: int;
      send input.counter, eRead, this;
      receive { case eValue: (x: int) { v = x; } }
      send input.counter, eWrite, v + 1;
      send input.owner, eDone;
    }
  }
}`&&i===`event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`,onClick:()=>D(h),children:`Read then write`}),(0,o.jsx)(`button`,{"aria-pressed":e===`machine Counter {
  var value: int;
  start state Serving {
    on eIncrement do { value = value + 1; }
    on eGet do (m: Main) { send m, eValue, value; }
  }
}

machine Client {
  start state Run {
    entry (input: (counter: Counter, owner: Main)) {
      send input.counter, eIncrement;
      send input.owner, eDone;
    }
  }
}`&&i===`event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`,onClick:()=>D(g),children:`Atomic increment`})]}),(0,o.jsxs)(`div`,{className:`mini-run-controls`,children:[v&&(0,o.jsx)(`span`,{className:v.exitCode===0?`mini-pass`:`mini-error`,role:`status`,children:M}),b&&(0,o.jsx)(`span`,{className:`mini-error`,role:`status`,children:`Check could not run`}),(0,o.jsx)(`button`,{className:`button primary`,onClick:O,disabled:f,children:f?`Checking…`:`Run model`}),f&&(0,o.jsx)(`button`,{onClick:E,children:`Stop`})]})]}),(0,o.jsxs)(`div`,{className:`mini-workspace`,children:[(0,o.jsxs)(`div`,{className:`mini-source`,children:[(0,o.jsxs)(`div`,{className:`mini-filebar`,children:[(0,o.jsxs)(`div`,{role:`group`,"aria-label":`P source files`,children:[(0,o.jsx)(`button`,{"aria-pressed":c===`model`,onClick:()=>l(`model`),children:`Model.p`}),(0,o.jsx)(`button`,{"aria-pressed":c===`setup`,onClick:()=>l(`setup`),children:`Test.p`})]}),(0,o.jsx)(`button`,{onClick:()=>d(!u),children:u?`Done editing`:`Edit P code`})]}),u?(0,o.jsx)(`textarea`,{ref:w,className:`mini-editor`,"aria-label":`Edit ${c===`model`?`Model.p`:`Test.p`}`,spellCheck:!1,value:T,onChange:e=>{E(),c===`model`?r(e.target.value):s(e.target.value)}}):(0,o.jsx)(`div`,{className:`mini-code`,role:`region`,"aria-label":`Read ${c===`model`?`Model.p`:`Test.p`}`,tabIndex:0,children:(0,o.jsx)(m,{lang:`p`,children:T})})]}),(0,o.jsxs)(`div`,{className:`mini-result`,children:[(0,o.jsx)(`h3`,{children:`Two increments should leave 2.`}),(0,o.jsx)(m,{lang:`p`,children:`assert final == 2;`}),(0,o.jsx)(`p`,{children:`Each client runs independently. The checker chooses which message is delivered next.`}),(0,o.jsxs)(`p`,{children:[(0,o.jsx)(`b`,{children:`Read then write`}),` leaves a gap for the other client to run. `,(0,o.jsx)(`b`,{children:`Atomic increment`}),` updates the value in a single handler.`]}),i!==`event eIncrement;
event eRead: Client;          // ask the counter for its value
event eValue: int;            // the counter's reply
event eWrite: int;            // overwrite the counter
event eDone;                  // a client finished
event eGet: Main;             // the test asks for the final value

machine Main {
  var counter: Counter;
  var done: int;
  start state Init {
    entry {
      counter = new Counter();
      new Client((counter = counter, owner = this));
      new Client((counter = counter, owner = this));
    }
    on eDone do {
      done = done + 1;
      if (done == 2) {
        send counter, eGet, this;
        goto Verify;
      }
    }
  }
  state Verify {
    on eValue do (final: int) {
      assert final == 2, format("expected 2 increments, counter is {0}", final);
    }
  }
}

test tcTwoClients [main=Main]: { Main, Counter, Client };
`&&(0,o.jsx)(`p`,{className:`mini-edited-note`,children:`Test.p has been edited. The checker will use your edited assertions, not necessarily the starting property above.`}),(0,o.jsx)(`div`,{className:`mini-status`,role:`status`,"aria-live":`polite`,children:b?(0,o.jsx)(`p`,{className:`mini-error`,children:b}):v?(0,o.jsxs)(o.Fragment,{children:[(0,o.jsx)(`h4`,{className:v.exitCode===0?`mini-pass`:`mini-error`,children:M}),(0,o.jsx)(`p`,{children:v.exitCode===0?`Read the checker output for search completion and warnings. This checks the model within its bounds, not your implementation.`:v.bug?`An execution broke a check. Inspect the output and trace below.`:`Read the diagnostics below, edit the code, and run again.`})]}):(0,o.jsx)(`p`,{children:f?`Loading the checker and exploring event orderings…`:`Run the model, inspect the result, then try the other design or edit the P code.`})})]})]}),v&&(0,o.jsxs)(`div`,{className:`mini-output`,children:[(0,o.jsxs)(`details`,{open:!0,children:[(0,o.jsx)(`summary`,{children:`Checker output`}),(0,o.jsx)(`pre`,{children:v.console})]}),v.traceText&&(0,o.jsxs)(`details`,{children:[(0,o.jsx)(`summary`,{children:`Counterexample trace`}),(0,o.jsx)(`pre`,{children:v.traceText})]})]}),(0,o.jsxs)(`div`,{className:`mini-footer`,children:[(0,o.jsx)(`span`,{children:`2-second exploration budget · mutation checks off for this demo`}),(0,o.jsx)(`a`,{href:j,children:A?`Open these files in the full Playground →`:`Project too large for a link — open Playground and copy your files →`})]}),(0,o.jsx)(`noscript`,{children:(0,o.jsxs)(`p`,{children:[`Enable JavaScript to edit and run this model, or follow the `,(0,o.jsx)(`a`,{href:`./docs/first-model.html`,children:`tutorial`}),`.`]})})]})}export{v as MiniPlayground};