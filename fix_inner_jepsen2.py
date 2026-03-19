import re

with open('src/jepsen/nemesis.rs', 'r') as f:
    nemesis_content = f.read()

# Fix max_skew_ms
nemesis_content = nemesis_content.replace('rand::random::<i64>() % max_skew_ms', 'rand::random::<i64>() % self.max_skew_ms')

# Fix ProcessFaultType::Crash -> ProcessFaultType::Kill
nemesis_content = nemesis_content.replace('ProcessFaultType::Crash', 'ProcessFaultType::Kill')


with open('src/jepsen/nemesis.rs', 'w') as f:
    f.write(nemesis_content)


with open('src/jepsen/workloads.rs', 'r') as f:
    workload_content = f.read()

# WorkloadType was not an enum it was an enum WorkloadType, and trait Workload!
# We renamed WorkloadType to Workload in workloads.rs, but Workload is already a trait in workloads.rs line 17.
# Let's revert renaming WorkloadType -> Workload in workloads.rs
workload_content = workload_content.replace('use super::{OperationType, TransactionOp, Workload};', 'use super::{OperationType, TransactionOp};')

# We need to change the function signatures back
workload_content = workload_content.replace('pub fn create_workload(workload_type: Workload) -> Box<dyn Workload>', 'pub fn create_workload(workload_type: crate::jepsen::production_tests::WorkloadType) -> Box<dyn Workload>')
workload_content = workload_content.replace('workload_type: Workload,', 'workload_type: crate::jepsen::production_tests::WorkloadType,')

# and replace Workload:: back to crate::jepsen::production_tests::WorkloadType::
workload_content = workload_content.replace('Workload::Register', 'crate::jepsen::production_tests::WorkloadType::Register')
workload_content = workload_content.replace('Workload::Set', 'crate::jepsen::production_tests::WorkloadType::Set')
workload_content = workload_content.replace('Workload::Append', 'crate::jepsen::production_tests::WorkloadType::Append')
workload_content = workload_content.replace('Workload::ReadWrite', 'crate::jepsen::production_tests::WorkloadType::ReadWrite')
workload_content = workload_content.replace('Workload::Bank', 'crate::jepsen::production_tests::WorkloadType::Bank')
workload_content = workload_content.replace('Workload::Counter', 'crate::jepsen::production_tests::WorkloadType::Counter')
workload_content = workload_content.replace('Workload::List', 'crate::jepsen::production_tests::WorkloadType::List')

with open('src/jepsen/workloads.rs', 'w') as f:
    f.write(workload_content)


# And in tests/jepsen_tests.rs we should also use crate::jepsen::production_tests::WorkloadType instead of WorkloadType
with open('tests/jepsen_tests.rs', 'r') as f:
    tests_content = f.read()

tests_content = tests_content.replace('Workload::Register', 'rtdb::jepsen::production_tests::WorkloadType::Register')
tests_content = tests_content.replace('Workload::Bank', 'rtdb::jepsen::production_tests::WorkloadType::Bank')
tests_content = tests_content.replace('Workload::Counter', 'rtdb::jepsen::production_tests::WorkloadType::Counter')

with open('tests/jepsen_tests.rs', 'w') as f:
    f.write(tests_content)
