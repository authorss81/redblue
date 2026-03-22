use crate::error::{Error, Result};
use crate::value::Value;

pub fn call(function: &str, args: Vec<Value>) -> Result<Value> {
    match function {
        "PI" => Ok(Value::Number(std::f64::consts::PI)),
        "E" => Ok(Value::Number(std::f64::consts::E)),
        "TAU" | "TWO_PI" => Ok(Value::Number(std::f64::consts::TAU)),
        "SQRT2" => Ok(Value::Number(std::f64::consts::SQRT_2)),
        "SQRT1_2" => Ok(Value::Number(std::f64::consts::FRAC_1_SQRT_2)),
        "LN2" => Ok(Value::Number(std::f64::consts::LN_2)),
        "LN10" => Ok(Value::Number(std::f64::consts::LN_10)),
        "LOG2_E" => Ok(Value::Number(std::f64::consts::LOG2_E)),
        "LOG10_E" => Ok(Value::Number(std::f64::consts::LOG10_E)),
        
        "abs" => abs(args),
        "floor" => floor(args),
        "ceil" => ceil(args),
        "round" => round(args),
        "trunc" => trunc(args),
        "fract" => fract(args),
        "sqrt" => sqrt(args),
        "cbrt" => cbrt(args),
        "pow" => pow(args),
        "hypot" => hypot(args),
        
        "sin" => sin(args),
        "cos" => cos(args),
        "tan" => tan(args),
        "asin" => asin(args),
        "acos" => acos(args),
        "atan" => atan(args),
        "atan2" => atan2(args),
        "sinh" => sinh(args),
        "cosh" => cosh(args),
        "tanh" => tanh(args),
        "asinh" => asinh(args),
        "acosh" => acosh(args),
        "atanh" => atanh(args),
        
        "ln" | "log" => ln(args),
        "log2" => log2(args),
        "log10" => log10(args),
        "exp" => exp(args),
        "exp2" => exp2(args),
        
        "min" => min(args),
        "max" => max(args),
        "clamp" => clamp(args),
        "degrees" => degrees(args),
        "radians" => radians(args),
        "sign" => sign(args),
        "is_nan" | "isNan" => is_nan(args),
        "is_infinite" | "isInfinite" => is_infinite(args),
        "is_finite" | "isFinite" => is_finite(args),
        
        "random" => random(args),
        "random_int" | "randomInt" => random_int(args),
        "random_float" | "randomFloat" => random_float(args),
        
        _ => Err(Error::Runtime(format!("Unknown math function '{}'", function))),
    }
}

fn abs(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.abs()))
}

fn floor(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.floor()))
}

fn ceil(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.ceil()))
}

fn round(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.round()))
}

fn trunc(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.trunc()))
}

fn fract(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.fract()))
}

fn sqrt(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.sqrt()))
}

fn cbrt(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.cbrt()))
}

fn pow(args: Vec<Value>) -> Result<Value> {
    let base = expect_number(args.first())?;
    let exp = expect_number(args.get(1))?;
    Ok(Value::Number(base.powf(exp)))
}

fn hypot(args: Vec<Value>) -> Result<Value> {
    let x = expect_number(args.first())?;
    let y = expect_number(args.get(1))?;
    Ok(Value::Number(x.hypot(y)))
}

fn sin(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.sin()))
}

fn cos(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.cos()))
}

fn tan(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.tan()))
}

fn asin(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.asin()))
}

fn acos(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.acos()))
}

fn atan(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.atan()))
}

fn atan2(args: Vec<Value>) -> Result<Value> {
    let y = expect_number(args.first())?;
    let x = expect_number(args.get(1))?;
    Ok(Value::Number(y.atan2(x)))
}

fn sinh(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.sinh()))
}

fn cosh(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.cosh()))
}

fn tanh(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.tanh()))
}

fn asinh(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.asinh()))
}

fn acosh(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.acosh()))
}

fn atanh(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.atanh()))
}

fn ln(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.ln()))
}

fn log2(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.log2()))
}

fn log10(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.log10()))
}

fn exp(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.exp()))
}

fn exp2(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.exp2()))
}

fn min(args: Vec<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Runtime("min requires at least one argument".to_string()));
    }
    
    let mut min_val = expect_number(Some(&args[0]))?;
    for arg in args.iter().skip(1) {
        let n = expect_number(Some(arg))?;
        if n < min_val {
            min_val = n;
        }
    }
    Ok(Value::Number(min_val))
}

fn max(args: Vec<Value>) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Runtime("max requires at least one argument".to_string()));
    }
    
    let mut max_val = expect_number(Some(&args[0]))?;
    for arg in args.iter().skip(1) {
        let n = expect_number(Some(arg))?;
        if n > max_val {
            max_val = n;
        }
    }
    Ok(Value::Number(max_val))
}

fn clamp(args: Vec<Value>) -> Result<Value> {
    let val = expect_number(args.first())?;
    let min = expect_number(args.get(1))?;
    let max = expect_number(args.get(2))?;
    
    if val < min {
        Ok(Value::Number(min))
    } else if val > max {
        Ok(Value::Number(max))
    } else {
        Ok(Value::Number(val))
    }
}

fn degrees(args: Vec<Value>) -> Result<Value> {
    let radians = expect_number(args.first())?;
    Ok(Value::Number(radians.to_degrees()))
}

fn radians(args: Vec<Value>) -> Result<Value> {
    let degrees = expect_number(args.first())?;
    Ok(Value::Number(degrees.to_radians()))
}

fn sign(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::Number(n.signum()))
}

fn is_nan(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::YesNo(n.is_nan()))
}

fn is_infinite(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::YesNo(n.is_infinite()))
}

fn is_finite(args: Vec<Value>) -> Result<Value> {
    let n = expect_number(args.first())?;
    Ok(Value::YesNo(n.is_finite()))
}

fn random(_args: Vec<Value>) -> Result<Value> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    Ok(Value::Number((seed % 1000) as f64 / 1000.0))
}

fn random_int(args: Vec<Value>) -> Result<Value> {
    let min = args.first().map(expect_number).unwrap_or(Ok(0.0))?;
    let max = args.get(1).map(expect_number).unwrap_or(Ok(100.0))?;
    
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let range = (max - min + 1.0) as u64;
    let result = min as i64 + ((seed % range) as i64);
    Ok(Value::Number(result as f64))
}

fn random_float(args: Vec<Value>) -> Result<Value> {
    let min = args.first().map(expect_number).unwrap_or(Ok(0.0))?;
    let max = args.get(1).map(expect_number).unwrap_or(Ok(1.0))?;
    
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let fraction = (seed % 1000000) as f64 / 1000000.0;
    Ok(Value::Number(min + fraction * (max - min)))
}

fn expect_number(value: Option<&Value>) -> Result<f64> {
    match value {
        Some(Value::Number(n)) => Ok(*n),
        Some(v) => Err(Error::Runtime(format!("Expected number, got {}", v))),
        None => Err(Error::Runtime("Missing argument".to_string())),
    }
}
