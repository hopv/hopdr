exception TrueExc
exception FalseExc
exception IntegerOverflow
exception RecursionExceeded

(* trace *)
type id = int
type value = int

type trace =
  | TEmpty
  | TConj of id * trace
  | TDisj of trace * trace
  | TUniv of value * trace
  | TApp of string * value list * trace

let mk_empty_trace () = TEmpty
let mk_conj x t = TConj (x, t)
let mk_disj t1 t2 = TDisj (t1, t2)
let mk_univ v t = TUniv (v, t)
let mk_app f vs t = TApp (f, vs, t)

let print_trace t =
  let rec go t =
    match t with
    | TEmpty -> Printf.printf "()"
    | TConj (x, t) ->
        Printf.printf "(conj %d " x;
        go t;
        Printf.printf ")"
    | TDisj (t1, t2) ->
        Printf.printf "(disj ";
        go t1;
        Printf.printf " ";
        go t2;
        Printf.printf ")"
    | TUniv (v, t) ->
        Printf.printf "(univ %d " v;
        go t;
        Printf.printf ")"
    | TApp (f, vs, t) ->
        Printf.printf "(app %s (" f;
        if vs <> [] then
          vs |> List.rev |> List.iter (fun v -> Printf.printf "%d " v);
        Printf.printf ") ";
        go t;
        Printf.printf ")"
  in
  Printf.printf "[[trace]]\n";
  go t;
  Printf.printf "\n"

(* stats *)
type t_counter = {
  mutable retry : int;
  mutable recursion : int;
  mutable rand_int : int;
}

let counter = { retry = 0; recursion = 0; rand_int = 0 }

let print_counter_stats () =
  Printf.printf "[[counter stats]]\n";
  Printf.printf "retry: %d\n" counter.retry;
  Printf.printf "recursion: %d\n" counter.recursion;
  Printf.printf "rand_int: %d\n" counter.rand_int

(* random generator *)
let check_mx = ref 100000
let check_mn = ref (-100000)

let set_min () =
  check_mx := 2;
  check_mn := -1

let set_small () =
  check_mx := 6;
  check_mn := -5

let set_med () =
  check_mx := 151;
  check_mn := -150

let set_large () =
  check_mx := 100000;
  check_mn := -100000

let n_recursion = ref 0
let n_recursion_limit = ref 1000

let hopdr_count_recursion () =
  n_recursion := !n_recursion + 1;
  if !n_recursion > !n_recursion_limit then raise RecursionExceeded;
  counter.recursion <- counter.recursion + 1

let set_n_recursion_limit n = n_recursion_limit := n
let reset_n_recursion () = n_recursion := 0

let event_integer_overflow () =
  if !check_mx > 10 then check_mx := !check_mx / 2;
  if !check_mn < -10 then check_mn := !check_mn / 2

let event_stack_overflow () = ()

let rand_int (x, y) =
  counter.rand_int <- counter.rand_int + 1;
  let diff = !check_mx - !check_mn in
  let mn, mx =
    match (x, y) with
    | Some x, Some y -> (x, y)
    | Some x, None -> (x, x + diff)
    | None, Some y -> (y - diff, y)
    | None, None -> (!check_mn, !check_mx)
  in
  Random.int (mx - mn) + mn

let check_overflow f x y =
  try f x y with Invalid_argument _ -> raise IntegerOverflow

let ( + ) a b =
  if a > 0 && b > 0 && a > max_int - b then raise IntegerOverflow
  else if a < 0 && b < 0 && a < min_int - b then raise IntegerOverflow
  else a + b

let ( - ) a b =
  if b > 0 && a < min_int + b then raise IntegerOverflow
  else if b < 0 && a > max_int + b then raise IntegerOverflow
  else a - b

let ( * ) a b =
  if a = 0 || b = 0 then 0
  else if a = -1 && b = min_int then raise IntegerOverflow
  else if b = -1 && a = min_int then raise IntegerOverflow
  else if a > 0 && b > 0 && a > max_int / b then raise IntegerOverflow
  else if a < 0 && b < 0 && a < max_int / b then raise IntegerOverflow
  else if a > 0 && b < 0 && b < min_int / a then raise IntegerOverflow
  else if a < 0 && b > 0 && a < min_int / b then raise IntegerOverflow
  else a * b

let ( mod ) a b =
  let a' = a mod b in
  if a' < 0 then a' + b else a'

let loop f n =
  for i = 1 to n do
    Printf.printf "epoch %d...\n" i;
    counter.retry <- counter.retry + 1;
    reset_n_recursion ();
    try
      (* if it terminates, it means that the program is *NOT* safe *)
      let () = f () in
      raise FalseExc
    with
    | IntegerOverflow ->
        Printf.printf "int overflow";
        event_integer_overflow ()
    | Stack_overflow ->
        Printf.printf "stack overflow\n";
        event_stack_overflow ()
    | RecursionExceeded -> ()
    | TrueExc -> ()
  done

let rec hopdr_main f fail =
  let n_recs = [ 1000; 10000; 100000; 1000000; 10000000000 ] in
  let configs = [ set_min; set_small; set_med; set_large ] in
  try
    List.iter
      (fun n_rec ->
        List.iter
          (fun config ->
            set_n_recursion_limit n_rec;
            config ();
            loop f 1000)
          configs)
      n_recs;
    print_counter_stats ();
    if fail () then hopdr_main f fail
  with e ->
    print_counter_stats ();
    raise e

(*** The program body starts here! ***)
