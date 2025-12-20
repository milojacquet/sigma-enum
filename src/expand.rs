use crate::nice_type::NiceTypeLit;
use syn::Expr;
use syn::ExprRange;
use syn::spanned::Spanned;

fn extract_expansion_range(expr: &ExprRange) -> syn::Result<Vec<NiceTypeLit>> {
    let start = match &expr.start {
        Some(expr) => match &**expr {
            Expr::Lit(expr) => match &expr.lit {
                syn::Lit::Int(lit_int) => lit_int,
                _ => return Err(syn::Error::new(expr.span(), "bad literal")),
            },
            _ => return Err(syn::Error::new(expr.span(), "not literal")),
        },
        None => return Err(syn::Error::new(expr.span(), "no start")),
    };
    let end = match &expr.end {
        Some(expr) => match &**expr {
            Expr::Lit(expr) => match &expr.lit {
                syn::Lit::Int(lit_int) => lit_int,
                _ => return Err(syn::Error::new(expr.span(), "bad literal")),
            },
            _ => return Err(syn::Error::new(expr.span(), "not literal")),
        },
        None => return Err(syn::Error::new(expr.span(), "no end")),
    };

    if let Ok(start) = start.base10_parse::<i128>()
        && start < 1 << 126
    {
        let end: i128 = end.base10_parse()?;
        if end - start > 65536 {
            return Err(syn::Error::new(
                expr.span(),
                format!("range too large ({} > 65536)", end - start),
            ));
        }
        Ok(match expr.limits {
            syn::RangeLimits::HalfOpen(_) => (start..end)
                .map(|n| NiceTypeLit::Int(n.to_string()))
                .collect(),
            syn::RangeLimits::Closed(_) => (start..=end)
                .map(|n| NiceTypeLit::Int(n.to_string()))
                .collect(),
        })
    } else {
        let start: u128 = start.base10_parse()?;
        let end: u128 = end.base10_parse()?;
        if end - start > 65536 {
            return Err(syn::Error::new(
                expr.span(),
                format!("range too large ({} > 65536)", end - start),
            ));
        }
        Ok(match expr.limits {
            syn::RangeLimits::HalfOpen(_) => (start..end)
                .map(|n| NiceTypeLit::Int(n.to_string()))
                .collect(),
            syn::RangeLimits::Closed(_) => (start..=end)
                .map(|n| NiceTypeLit::Int(n.to_string()))
                .collect(),
        })
    }
}

pub fn extract_expansion(expr: &Expr) -> syn::Result<Vec<NiceTypeLit>> {
    match expr {
        Expr::Lit(expr) => Ok(vec![NiceTypeLit::from_lit(&expr.lit)?]),
        Expr::Range(expr) => extract_expansion_range(expr),
        Expr::Array(expr) => {
            let mut out = Vec::new();
            for expr in &expr.elems {
                match expr {
                    Expr::Lit(expr) => out.push(NiceTypeLit::from_lit(&expr.lit)?),
                    Expr::Range(expr) => out.extend(extract_expansion_range(&expr)?),
                    _ => return Err(syn::Error::new(expr.span(), "invalid value")),
                }
            }
            Ok(out)
        }
        _ => Err(syn::Error::new(expr.span(), "invalid value")),
    }
}
