use crate::error::{Error, Result};
use crate::value::Value;

pub fn call(function: &str, args: Vec<Value>) -> Result<Value> {
    match function {
        "length" | "len" => length(args),
        "push" => push(args),
        "pop" => pop(args),
        "shift" => shift(args),
        "unshift" => unshift(args),
        "get" | "at" => get(args),
        "set" => set(args),
        "first" => first(args),
        "last" => last(args),
        "slice" => slice(args),
        "splice" => splice(args),
        "concat" => concat(args),
        "reverse" => reverse(args),
        "sort" => sort(args),
        "sort_by" | "sortBy" => sort_by(args),
        "map" => list_map(args),
        "filter" => filter(args),
        "reduce" => reduce(args),
        "find" => find(args),
        "find_index" | "findIndex" => find_index(args),
        "includes" => includes(args),
        "index_of" | "indexOf" => index_of(args),
        "join" => join(args),
        "every" => every(args),
        "some" => some(args),
        "flat" => flat(args),
        "flat_map" | "flatMap" => flat_map(args),
        "fill" => fill(args),
        "copy_within" | "copyWithin" => copy_within(args),
        "is_empty" | "isEmpty" => is_empty(args),
        "count" => count(args),
        "sum" => sum(args),
        "product" => product(args),
        "average" | "avg" => average(args),
        "min" => min(args),
        "max" => max(args),
        "unique" => unique(args),
        "chunk" => chunk(args),
        "zip" => zip(args),
        "transpose" => transpose(args),
        _ => Err(Error::Runtime(format!("Unknown list function '{}'", function))),
    }
}

fn length(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    Ok(Value::Number(list.len() as f64))
}

fn push(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?.to_vec();
    let item = args.get(1).cloned().unwrap_or(Value::Nothing);
    let mut result = list;
    result.push(item);
    Ok(Value::List(result))
}

fn pop(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?.to_vec();
    match list.pop() {
        Some(v) => Ok(v),
        None => Ok(Value::Nothing),
    }
}

fn shift(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?.to_vec();
    match list.into_iter().next() {
        Some(v) => Ok(v),
        None => Ok(Value::Nothing),
    }
}

fn unshift(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?.to_vec();
    let item = args.get(1).cloned().unwrap_or(Value::Nothing);
    let mut result = vec![item];
    result.extend(list);
    Ok(Value::List(result))
}

fn get(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let index = expect_number(args.get(1))? as i64;
    
    let len = list.len() as i64;
    let idx = if index < 0 { len + index } else { index };
    
    match list.get(idx as usize) {
        Some(v) => Ok(v.clone()),
        None => Ok(Value::Nothing),
    }
}

fn set(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?.to_vec();
    let index = expect_number(args.get(1))? as usize;
    let value = args.get(2).cloned().unwrap_or(Value::Nothing);
    
    let mut result = list;
    if index < result.len() {
        result[index] = value;
    }
    Ok(Value::List(result))
}

fn first(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    Ok(list.first().cloned().unwrap_or(Value::Nothing))
}

fn last(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    Ok(list.last().cloned().unwrap_or(Value::Nothing))
}

fn slice(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let start = args.get(1).map(|v| expect_number(Some(v)).unwrap_or(0.0) as usize).unwrap_or(0);
    let end = args.get(2).map(|v| expect_number(Some(v)).unwrap_or(list.len() as f64) as usize).unwrap_or(list.len());
    
    let start = start.min(list.len());
    let end = end.min(list.len());
    
    Ok(Value::List(list[start..end].to_vec()))
}

fn splice(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("splice not implemented yet".to_string()))
}

fn concat(args: Vec<Value>) -> Result<Value> {
    let mut result = expect_list(args.first())?.to_vec();
    
    for arg in args.iter().skip(1) {
        if let Value::List(items) = arg {
            result.extend(items.clone());
        }
    }
    
    Ok(Value::List(result))
}

fn reverse(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?.to_vec();
    let mut result = list;
    result.reverse();
    Ok(Value::List(result))
}

fn sort(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?.to_vec();
    let mut result = list;
    result.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
    Ok(Value::List(result))
}

fn sort_by(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("sort_by not implemented yet".to_string()))
}

fn list_map(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let _func = args.get(1);
    
    Err(Error::Runtime("map requires a function - use list.map()".to_string()))
}

fn filter(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let _func = args.get(1);
    
    Err(Error::Runtime("filter requires a function - use list.filter()".to_string()))
}

fn reduce(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let initial = args.get(1).cloned().unwrap_or(Value::Nothing);
    let _func = args.get(2);
    
    Err(Error::Runtime("reduce requires a function - use list.reduce()".to_string()))
}

fn find(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("find requires a predicate function".to_string()))
}

fn find_index(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("find_index requires a predicate function".to_string()))
}

fn includes(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let item = args.get(1).cloned().unwrap_or(Value::Nothing);
    Ok(Value::YesNo(list.contains(&item)))
}

fn index_of(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let item = args.get(1).cloned().unwrap_or(Value::Nothing);
    
    for (i, v) in list.iter().enumerate() {
        if v == &item {
            return Ok(Value::Number(i as f64));
        }
    }
    Ok(Value::Number(-1.0))
}

fn join(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let separator = args.get(1).map(|v| v.to_string()).unwrap_or_default();
    
    let result: Vec<String> = list.iter().map(|v| v.to_string()).collect();
    Ok(Value::Text(result.join(&separator)))
}

fn every(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("every requires a predicate function".to_string()))
}

fn some(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("some requires a predicate function".to_string()))
}

fn flat(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let depth = args.get(1).map(|v| expect_number(Some(v)).unwrap_or(1.0) as usize).unwrap_or(1);
    
    fn flatten(list: &[Value], depth: usize) -> Vec<Value> {
        if depth == 0 {
            return list.to_vec();
        }
        
        let mut result = Vec::new();
        for item in list {
            match item {
                Value::List(items) => result.extend(flatten(items, depth - 1)),
                _ => result.push(item.clone()),
            }
        }
        result
    }
    
    Ok(Value::List(flatten(&list, depth)))
}

fn flat_map(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("flat_map requires a function".to_string()))
}

fn fill(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?.to_vec();
    let value = args.get(1).cloned().unwrap_or(Value::Nothing);
    
    Ok(Value::List(vec![value; list.len()]))
}

fn copy_within(_args: Vec<Value>) -> Result<Value> {
    Err(Error::Runtime("copy_within not implemented yet".to_string()))
}

fn is_empty(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    Ok(Value::YesNo(list.is_empty()))
}

fn count(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    Ok(Value::Number(list.len() as f64))
}

fn sum(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let mut total = 0.0f64;
    
    for item in list {
        if let Value::Number(n) = item {
            total += n;
        }
    }
    
    Ok(Value::Number(total))
}

fn product(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let mut total = 1.0f64;
    
    for item in list {
        if let Value::Number(n) = item {
            total *= n;
        }
    }
    
    Ok(Value::Number(total))
}

fn average(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    if list.is_empty() {
        return Ok(Value::Nothing);
    }
    
    let mut total = 0.0f64;
    let mut count = 0.0f64;
    
    for item in list {
        if let Value::Number(n) = item {
            total += n;
            count += 1.0;
        }
    }
    
    if count == 0.0 {
        return Ok(Value::Nothing);
    }
    
    Ok(Value::Number(total / count))
}

fn min(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    
    let mut min_val: Option<f64> = None;
    
    for item in list {
        if let Value::Number(n) = item {
            match min_val {
                None => min_val = Some(n),
                Some(m) if n < m => min_val = Some(n),
                _ => {},
            }
        }
    }
    
    match min_val {
        Some(v) => Ok(Value::Number(v)),
        None => Ok(Value::Nothing),
    }
}

fn max(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    
    let mut max_val: Option<f64> = None;
    
    for item in list {
        if let Value::Number(n) = item {
            match max_val {
                None => max_val = Some(n),
                Some(m) if n > m => max_val = Some(n),
                _ => {},
            }
        }
    }
    
    match max_val {
        Some(v) => Ok(Value::Number(v)),
        None => Ok(Value::Nothing),
    }
}

fn unique(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    
    for item in list {
        if seen.insert(item.to_string()) {
            result.push(item);
        }
    }
    
    Ok(Value::List(result))
}

fn chunk(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    let size = args.get(1).map(|v| expect_number(Some(v)).unwrap_or(1.0) as usize).unwrap_or(1).max(1);
    
    let chunks: Vec<Value> = list.chunks(size)
        .map(|chunk| Value::List(chunk.to_vec()))
        .collect();
    
    Ok(Value::List(chunks))
}

fn zip(args: Vec<Value>) -> Result<Value> {
    if args.is_empty() {
        return Ok(Value::List(Vec::new()));
    }
    
    let lists: Vec<Vec<Value>> = args.iter()
        .map(|v| expect_list(Some(v)))
        .collect::<Result<Vec<_>>>()?;
    
    let min_len = lists.iter().map(|l| l.len()).min().unwrap_or(0);
    let mut result = Vec::new();
    
    for i in 0..min_len {
        let tuple: Vec<Value> = lists.iter().map(|l| l[i].clone()).collect();
        result.push(Value::List(tuple));
    }
    
    Ok(Value::List(result))
}

fn transpose(args: Vec<Value>) -> Result<Value> {
    let list = expect_list(args.first())?;
    
    let matrix: Vec<Vec<Value>> = list.iter()
        .map(|v| expect_list(Some(v)))
        .collect::<Result<Vec<_>>>()?;
    
    if matrix.is_empty() || matrix[0].is_empty() {
        return Ok(Value::List(Vec::new()));
    }
    
    let rows = matrix.len();
    let cols = matrix[0].len();
    let mut result = Vec::new();
    
    for c in 0..cols {
        let mut row = Vec::new();
        for r in 0..rows {
            if c < matrix[r].len() {
                row.push(matrix[r][c].clone());
            }
        }
        result.push(Value::List(row));
    }
    
    Ok(Value::List(result))
}

fn expect_list(value: Option<&Value>) -> Result<Vec<Value>> {
    match value {
        Some(Value::List(items)) => Ok(items.clone()),
        Some(v) => Err(Error::Runtime(format!("Expected list, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}

fn expect_number(value: Option<&Value>) -> Result<f64> {
    match value {
        Some(Value::Number(n)) => Ok(*n),
        Some(v) => Err(Error::Runtime(format!("Expected number, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}
