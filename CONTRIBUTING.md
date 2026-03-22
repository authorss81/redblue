# Contributing to Redblue

Thank you for your interest in contributing to Redblue!

## Code of Conduct

By participating, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before contributing.

## Ways to Contribute

### Code Contributions

- Fix bugs in the compiler or standard library
- Implement new features
- Improve performance
- Add test coverage
- Documentation improvements

### Non-Code Contributions

- Report bugs and suggest features
- Write tutorials and examples
- Help with translations
- Review pull requests
- Participate in design discussions

## Development Setup

### Prerequisites

- Rust 1.70 or later
- Git
- A text editor (VS Code recommended with Rust Analyzer)

### Getting Started

1. **Fork the repository**
   ```bash
   git clone https://github.com/YOUR_USERNAME/redblue.git
   cd redblue
   ```

2. **Add upstream remote**
   ```bash
   git remote add upstream https://github.com/redblue/lang.git
   ```

3. **Create a development branch**
   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/your-bug-fix
   ```

4. **Make your changes**
   ```bash
   # Edit code...
   
   # Run tests
   cargo test
   
   # Run linter
   cargo fmt
   cargo clippy
   ```

5. **Commit your changes**
   ```bash
   git add .
   git commit -m "feat: add awesome new feature"
   ```

6. **Push and create PR**
   ```bash
   git push origin feature/your-feature-name
   ```

## Coding Standards

### Style Guide

- Use 4 spaces for indentation
- Maximum line length: 100 characters
- Use meaningful variable names (English)
- Comment complex logic, not obvious code
- One concept per line

### Rust Code

```rust
// Good
fn calculate_total(items: &[Item]) -> Result<f64, CalcError> {
    let mut sum = 0.0;
    for item in items {
        sum += item.price?;
    }
    Ok(sum)
}

// Bad
fn calc(i:&[Item])->Result<f64,CalcError>{
    let mut s=0.0;
    for x in i{s+=x.price?;}
    Ok(s)
}
```

### Redblue Code (for examples/tests)

```redblue
// Good
to calculate_total(items)
    set sum to 0
    for each item in items
        add item price to sum
    end
    give back sum
end

// Bad
to calc(i)
    set s to 0
    for x in i
        s=s+x.price
    end
    s
end
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add pattern matching support
fix: handle division by zero correctly
docs: update installation instructions
test: add tests for async functions
refactor: simplify type inference
perf: optimize bytecode generation
```

### Testing

- All new features must include tests
- Run the full test suite before submitting PR
- Aim for >80% code coverage on new code

```bash
# Run all tests
cargo test --all

# Run with coverage
cargo install cargo-llvm-cov
cargo llvm-cov

# Run specific test
cargo test test_name
```

## Pull Request Process

### Before Submitting

1. **Format code**
   ```bash
   cargo fmt --all
   ```

2. **Run linter**
   ```bash
   cargo clippy --all-targets -- -D warnings
   ```

3. **Run tests**
   ```bash
   cargo test --all
   ```

4. **Update documentation**
   - Update relevant docs
   - Add examples for new features
   - Update CHANGELOG.md

### PR Description

Include:
- Summary of changes
- Motivation/why this change
- Implementation details
- Testing done
- Screenshots (if UI changes)

### Review Process

1. Maintainers will review within 1 week
2. Address feedback promptly
3. Once approved, maintainer will merge

## Reporting Bugs

### Bug Report Template

```markdown
## Description
A clear description of the bug.

## Steps to Reproduce
1. Go to '...'
2. Run '...'
3. See error

## Expected Behavior
What should happen.

## Actual Behavior
What actually happens.

## Environment
- OS: [e.g., macOS 14.0]
- Redblue version: [e.g., 0.1.0]
- Rust version: [e.g., 1.75.0]

## Additional Context
Any other context about the problem.
```

## Suggesting Features

### Feature Request Template

```markdown
## Problem
Describe the problem this feature would solve.

## Proposed Solution
Describe your proposed solution.

## Use Cases
Who would use this and why?

## Alternatives Considered
Other solutions you've considered.

## Additional Context
Screenshots, mockups, or related issues.
```

## Style Guidelines for This Document

- Be welcoming and inclusive
- Use "you" to address contributors
- Provide concrete examples
- Link to relevant documentation
- Keep it concise but complete

## Questions?

- Open a [GitHub Discussion](https://github.com/redblue/lang/discussions)
- Join our [Discord](https://discord.gg/redblue)
- Check the [FAQ](docs/faq.md)

## Recognition

Contributors will be recognized in:
- CHANGELOG.md release notes
- CONTRIBUTORS.md file
- Project website (eventually)

Thank you for making Redblue better!
