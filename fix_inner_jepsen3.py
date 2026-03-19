import re

with open('src/jepsen/workloads.rs', 'r') as f:
    workload_content = f.read()

# I see production_tests is also missing from mod.rs maybe? Or perhaps I should just move WorkloadType into workloads.rs where it belongs, or it exists in production_tests.rs. Let's look if it's in production_tests.rs
# Wait, let's just make a new enum in workloads.rs since the trait is Workload.

workload_content = workload_content.replace('crate::jepsen::production_tests::WorkloadType::', 'WorkloadType::')
workload_content = workload_content.replace('crate::jepsen::production_tests::WorkloadType', 'WorkloadType')

new_enum = """
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkloadType {
    Register,
    Set,
    Append,
    ReadWrite,
    Bank,
    Counter,
    List,
}

pub trait Workload: Send + Sync {
"""
workload_content = workload_content.replace('pub trait Workload: Send + Sync {', new_enum)

with open('src/jepsen/workloads.rs', 'w') as f:
    f.write(workload_content)

# And similarly for tests_jepsen_tests.rs
with open('tests/jepsen_tests.rs', 'r') as f:
    tests_content = f.read()

tests_content = tests_content.replace('rtdb::jepsen::production_tests::WorkloadType::', 'rtdb::jepsen::workloads::WorkloadType::')

with open('tests/jepsen_tests.rs', 'w') as f:
    f.write(tests_content)
