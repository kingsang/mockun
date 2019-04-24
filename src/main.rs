use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;
use std::io::BufReader;
use std::fs::File;
use std::thread;
use std::sync::Arc;

const USAGE: &'static str = r#"
Usage:
  mockun [-p <port>] [-h <custom response header>] /path:/xxx/file ...

Example:
  mockun -p 6789 -h some-custom-response-header /aa:./response.json /aa/bb:/response.text
"#;

fn main() {
  let args_vec: Vec<String> = std::env::args().collect();
  let args = parse_args(&args_vec);

  let host = format!("127.0.0.1:{}", args.port_opt.unwrap_or("7878".to_string()));

  let _path_and_response_bodies = make_responses(&args.path_and_file_names);
  let all_paths = _path_and_response_bodies.iter().map(|pr| " 🎯 ".to_string() + pr.path.as_str()).collect::<Vec<String>>();
  let all_custom_headers = args.custom_headers.iter().map(|h| " ➕ ".to_string() + h.as_str()).collect::<Vec<String>>();

  println!("mockun start!!\n 👉 {}\npaths are ...\n{}\ncustom headers are ...\n{}", host, all_paths.join("\n"), all_custom_headers.join("\n"));

  // request handlerをマルチスレッドに実行するのでArcでwrap
  let path_and_response_bodies = Arc::new(_path_and_response_bodies);
  let custom_headers = Arc::new(args.custom_headers);

  let listener = TcpListener::bind(host).unwrap();
  for stream in listener.incoming() {
    let stream = stream.unwrap();
    let shared_prb = path_and_response_bodies.clone();
    let shared_ch = custom_headers.clone();
    thread::spawn(move || {
      handle_connection(stream, &shared_prb, &shared_ch);
    });
  }
  drop(listener);
}

/*
  リクエストハンドラーのclosure
*/
fn handle_connection(mut stream: TcpStream, path_and_responses: &Vec<PathAndResponse>, custom_headers: &Vec<String>) -> () {
  // requestヘッダのパース
  let mut _stream = BufReader::new(stream.try_clone().unwrap());
  let mut first_line = String::new();
  _stream.read_line(&mut first_line).unwrap();

  let mut params = first_line.split_whitespace();
  let _method = params.next();
  let path = params.next().unwrap();

  // リクエストパスとキャッシュ上の設定パスを比較して一致した物のレスポンスを返す
  let path_and_response_opt = &path_and_responses.iter().find(|pr| pr.path == path);

  let [body, content_type] = path_and_response_opt
    .map(|pr| [pr.response_body.clone(), pr.content_type.clone()])
    .unwrap_or_else(|| ["nothing response is set!".to_string(), "text/plain".to_string()]);

  let custom_headers_str = "Access-Control-Allow-Headers: Origin,Authorization,Accept,Content-Type,".to_string() + custom_headers.join(",").as_str();

  writeln!(stream, "HTTP/1.1 200 OK").unwrap();
  writeln!(stream, "Access-Control-Allow-Origin: *").unwrap();
  writeln!(stream, "Access-Control-Allow-Methods: GET,POST,PUT,DELETE,HEAD,OPTIONS").unwrap();
  writeln!(stream, "{}", custom_headers_str).unwrap();
  writeln!(stream, "Content-Type: {}; charset=UTF-8", content_type).unwrap();
  writeln!(stream, "Content-Length: {}", body.len()).unwrap();
  writeln!(stream, "Server: {}", "mockun").unwrap();
  writeln!(stream).unwrap();
  writeln!(stream, "{}", body).unwrap();
  stream.flush().unwrap();
}

/*
  path->file を path->response body に変換
  fileの拡張子からcontentTypeを決定する
  json,js,text,html以外ならとりあえずtext/plainとする
*/
fn make_responses(path_and_file_names: &Vec<PathAndFileName>) -> Vec<PathAndResponse> {
  path_and_file_names.iter().map(|pf| {
    let file_name = pf.file_name.clone();

    let file_extension = *file_name.split(".").collect::<Vec<&str>>().iter().last().unwrap_or(&"text");
    let content_type = match file_extension {
      "json" => "application/json",
      "js" => "application/javascript",
      "text" => "text/plain",
      "html" => "text/html",
      _ => "text/plain"
    }.to_string();

    // mapのclosureの中で ? 使うとclosureがResultを返すべき関数とみなされエラーになるので仕方なくいちいちpanicする事にする
    let mut contents = String::new();
    let mut file = File::open(file_name).unwrap_or_else(|err| panic!(err.to_string() + USAGE));
    file.read_to_string(&mut contents).unwrap_or_else(|err| panic!(err.to_string() + USAGE));

    PathAndResponse {
      path: pf.path.clone(),
      response_body: contents,
      content_type,
    }
  }).collect()
}

/*
  引数をパースしてArgs構造体に詰める
*/
fn parse_args(_args_vec: &Vec<String>) -> Args {
  let args_vec = normalize_args(&_args_vec);

  // portオプションのパース
  let (port_opt, args_vec_without_port) = extract_flg_and_value("-p", &args_vec);

  // カスタムヘッダーのパース
  let (custom_headers, args_vec_without_port_and_custom_headers) = parse_custom_headers(&args_vec_without_port);

  // リクエストパスとレスポンスのパース
  let path_and_file_names: Vec<PathAndFileName> = args_vec_without_port_and_custom_headers.iter().map(|pf| {
    let pair_vec: Vec<&str> = pf.split(":").collect();
    // /path:./filepath という形で入ってくるので2要素必須
    if pair_vec.len() != 2 { panic!(USAGE); }
    PathAndFileName {
      path: pair_vec.get(0).unwrap().to_string(),
      file_name: pair_vec.get(1).unwrap().to_string(),
    }
  }).collect();

  // リクエストパスとレスポンスが無い場合を捕捉
  if path_and_file_names.len() == 0 {
    panic!(USAGE);
  }

  Args { path_and_file_names, port_opt, custom_headers }
}

/*
  引数を扱いやすい形に変換
 「-p7777」と「-p 7777」の指定両方に対応するために
 「[-p,7777,-h,example]」みたいな配列の形に揃える
*/
fn normalize_args(_args_vec: &Vec<String>) -> Vec<String> {
  let mut args_vec = _args_vec.clone();

  // 最初の要素は実行binaryパスが入るので除外
  args_vec.remove(0);

  args_vec.into_iter().flat_map(|arg| {
    if arg.starts_with("-p") || arg.starts_with("-h") {
      let (l, r) = arg.split_at(2);
      vec![
        vec![l.to_string()],
        if r == "" { vec![] } else { vec![r.to_string()] }
      ].into_iter().flatten().collect::<Vec<String>>()
    } else {
      vec![arg]
    }
  }).collect::<Vec<String>>()
}

/*
  引数群をcustom headersとそれ以外に分割
*/
fn parse_custom_headers(args_vec: &Vec<String>) -> (Vec<String>, Vec<String>) {
  let (custom_headers_opt, args_vec_without_header) = extract_flg_and_value("-h", &args_vec);
  let custom_headers = custom_headers_opt.map(|custom_headers| {
    let split: Vec<&str> = custom_headers.split(',').collect::<Vec<&str>>();
    split.into_iter()
      .map(|ch| ch.trim())
      .map(|ch| ch.to_string()).collect::<Vec<String>>()
  });
  (custom_headers.unwrap_or(vec![]), args_vec_without_header)
}

/*
  "flag value"の形で渡されるオプションと、それ以外に分割
  分割対象のflagを引数として受け取る
*/
fn extract_flg_and_value(target_flg: &str, _args_vec: &Vec<String>) -> (Option<String>, Vec<String>) {
  let mut args_vec = _args_vec.clone();
  let flg_index_opt = args_vec.iter().position(|r| r.starts_with(target_flg));

  // フラグのみが一番最後にある異常捕捉(drainがpanic起こすのでここ)
  flg_index_opt.iter().for_each(|idx| { if args_vec.len() == idx + 1 { panic!(USAGE); } });

  let value_opt = flg_index_opt.map(|index| {
    let flg_and_value = args_vec.drain(index..index + 2).collect::<Vec<String>>();
    flg_and_value.get(1).unwrap().to_string()
  });
  (value_opt, args_vec)
}

#[derive(Debug)]
struct Args {
  path_and_file_names: Vec<PathAndFileName>,
  port_opt: Option<String>,
  custom_headers: Vec<String>,
}

#[derive(Debug)]
struct PathAndFileName {
  path: String,
  file_name: String,
}

#[derive(Debug)]
struct PathAndResponse {
  path: String,
  response_body: String,
  content_type: String,
}