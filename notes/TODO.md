# TODO

## Compiler

- [ ] HIR Lowering
  - [ ] Stmt
    - [x] ConstVarDecl
    - [ ] TypeDecl
    - [ ] BindDecl
    - [ ] ProcDecl
    - [ ] FcnDecl
    - [ ] ProcessDecl
    - [ ] ExternalDecl
    - [ ] ForwardDecl
    - [ ] DeferredDecl
    - [ ] BodyDecl
    - [ ] ModuleDecl
    - [ ] ClassDecl
    - [ ] MonitorDecl
    - [x] AssignStmt
    - [ ] OpenStmt
    - [ ] CloseStmt
    - [X] PutStmt
    - [X] GetStmt
    - [ ] ReadStmt
    - [ ] WriteStmt
    - [ ] SeekStmt
    - [ ] TellStmt
    - [ ] ForStmt
    - [ ] LoopStmt
    - [ ] ExitStmt
    - [ ] IfStmt
    - [ ] CaseStmt
    - [x] BlockStmt
    - [ ] InvariantStmt
    - [ ] AssertStmt
    - [ ] CallStmt
    - [ ] ReturnStmt
    - [ ] ResultStmt
    - [ ] NewStmt
    - [ ] FreeStmt
    - [ ] TagStmt
    - [ ] ForkStmt
    - [ ] SignalStmt
    - [ ] PauseStmt
    - [ ] QuitStmt
    - [ ] BreakStmt
    - [ ] CheckednessStmt
    - [ ] PreStmt
    - [ ] InitStmt
    - [ ] PostStmt
    - [ ] HandlerStmt
    - [ ] InheritStmt
    - [ ] ImplementStmt
    - [ ] ImplementByStmt
    - [ ] ImportStmt
    - [ ] ExportStmt
  - [ ] Expr
    - [x] LiteralExpr
    - [ ] ObjClassExpr
    - [ ] InitExpr
    - [ ] NilExpr
    - [ ] SizeOfExpr
    - [x] BinaryExpr
    - [x] UnaryExpr
    - [x] ParenExpr
    - [x] NameExpr
    - [x] SelfExpr
    - [ ] FieldExpr
    - [ ] DerefExpr
    - [ ] CheatExpr
    - [ ] NatCheatExpr
    - [ ] ArrowExpr
    - [ ] IndirectExpr
    - [ ] BitsExpr
    - [ ] CallExpr
  - [ ] Type
    - [x] PrimType
    - [ ] NameType
    - [ ] RangeType
    - [ ] EnumType
    - [ ] ArrayType
    - [ ] SetType
    - [ ] RecordType
    - [ ] UnionType
    - [ ] PointerType
    - [ ] FcnType
    - [ ] ProcType
    - [ ] CollectionType
    - [ ] ConditionType
  - [ ] Preproc
    - [ ] PreprocIf stmt substitution
    - [ ] PreprocExpr evaluation
    - [ ] PreprocInclude insertion
      - [ ] Gather include files
- [ ] Import & dependency resolution
- [ ] Typeck
  - [ ] Stmt
    - [ ] ConstVar
    - [ ] Type
    - [ ] Bind
    - [ ] Proc
    - [ ] Fcn
    - [ ] Process
    - [ ] External
    - [ ] Forward
    - [ ] Deferred
    - [ ] Body
    - [ ] Module
    - [ ] Class
    - [ ] Monitor
    - [ ] Assign
    - [ ] Open
    - [ ] Close
    - [ ] Put
      - Waiting on non-put-able items to be lowered
    - [ ] Get
      - Waiting on non-get-able items to be lowered
    - [ ] Read
    - [ ] Write
    - [ ] Seek
    - [ ] Tell
    - [ ] For
    - [ ] Loop
    - [ ] Exit
    - [ ] If
    - [ ] Case
    - [ ] Block
    - [ ] Invariant
    - [ ] Assert
    - [ ] Call
    - [ ] Return
    - [ ] Result
    - [ ] New
    - [ ] Free
    - [ ] Tag
    - [ ] Fork
    - [ ] Signal
    - [ ] Pause
    - [ ] Quit
    - [ ] Break
    - [ ] Checkedness
    - [ ] Pre
    - [ ] Init
    - [ ] Post
    - [ ] Handler
  - [ ] Expr
    - [x] Literal
    - [ ] ObjClass
    - [ ] Init
    - [ ] Nil
    - [ ] SizeOf
    - [ ] Binary
      - [x] Arithmetic
      - [x] Bitwise
      - [x] Logical
      - [ ] Comparison
      - [ ] String concatenation
      - [ ] Set manipulation
    - [x] Unary
    - [x] Paren
    - [ ] Name
    - [ ] Self
    - [ ] Field
    - [ ] Deref
    - [ ] Cheat
    - [ ] NatCheat
    - [ ] Arrow
    - [ ] Indirect
    - [ ] Bits
    - [ ] Call
  - [ ] Type
    - [x] Prim (SeqLength)
    - [ ] Name
    - [ ] Range
    - [ ] Enum
    - [ ] Array
    - [ ] Set
    - [ ] Record
    - [ ] Union
    - [ ] Pointer
    - [ ] Fcn
    - [ ] Proc
    - [ ] Collection
    - [ ] Condition
- [ ] Const Eval (in `toc_analysis`)
  - [x] Cache evals of `ConstExpr`s
  - [x] Deferred eval of `const` vars
  - [ ] Evaluate all valid const ops
  - [ ] SizeOf structure size computation
- [ ] HIR Codegen
  - [ ] External functions support
- [ ] FileDb & FileId maps
- [x] Additional `ReportMessage` notes
  - [ ] Integration with `annotate-snippets`

### Potential

- [ ] MIR Lowering
- [ ] MIR Optimization
- [ ] MIR Codegen

### Const Operations

#### Arithmetic

- [ ] Add
  - [x] Over numbers
  - [ ] Over sets
  - [ ] Over char seqs
- [ ] Sub
  - [x] Over numbers
  - [ ] Over sets
- [ ] Mul
  - [x] Over numbers
  - [ ] Over sets
- [x] Div
- [x] RealDiv
- [x] Mod
- [x] Rem
- [x] Exp
- [x] Identity
- [x] Negate

#### Bitwise & Logic

- [x] And
- [x] Or
- [x] Xor
- [x] Shl
- [x] Shr
- [x] Imply
- [x] Not

#### Comparison

- [ ] Over numbers
  - [ ] Less
  - [ ] LessEq
  - [ ] Greater
  - [ ] GreaterEq
  - [ ] Equal
  - [ ] NotEqual
- [ ] Over sets
  - [ ] Less
  - [ ] LessEq
  - [ ] Greater
  - [ ] GreaterEq
  - [ ] Equal
  - [ ] NotEqual
- [ ] Over char seqs
  - [ ] Less
  - [ ] LessEq
  - [ ] Greater
  - [ ] GreaterEq
  - [ ] Equal
  - [ ] NotEqual

#### Type Conversion

- [ ] ord
- [ ] chr
- [ ] intreal
- [ ] natreal
- [ ] ceil
- [ ] round
- [ ] floor

#### Other

- [ ] pred
- [ ] succ
- [ ] lower
- [ ] upper (?)
- [ ] bits
- [ ] abs
- [ ] max
- [ ] min
- [ ] sign
- [ ] Set Constructor
- [ ] (BinaryOp) In

## LSP Client/Server

- [ ] Basic error reporting

## HIR Lowering Steps

### Group 1: Bare Bones

Putting together a simple program to work with for both HIR-based codegen & typeck.

- ConstVarDecl
- AssignStmt
- BlockStmt
- LiteralExpr
- BinaryExpr
- UnaryExpr
- ParenExpr
- NameExpr
- PrimType

### Group 2: Simple I/O

Interfacing visually with the Turing runtime.

- PutStmt
- GetStmt

### Group 3: Control Flow

Interesting codegen results from basic control flow.

- LoopStmt
- IfStmt
- ExitStmt
- CaseStmt
- ForStmt
