# Testing Cluster

저장소 루트에서 다음 품질 게이트를 실행한다.

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release --all-features
```

`cargo test --all-targets`는 라이브러리 단위/통합 테스트와 benchmark/example target 컴파일을 함께
확인한다. 주요 회귀 범위는 explicit endpoint와 교차선, canonical net 결정성, ERC 규칙과 자동
수정, MNA/RC transient, 명령 undo/redo, schema migration/원자 저장, PCB transform/ECO/DRC와
fabrication 차단이다.

성능 코드는 두 수준으로 제공한다.

```bash
cargo bench --bench performance
CLUSTER_PERF_SAMPLES=21 cargo run --release --example performance_probe
```

Criterion benchmark는 경로별 상대 성능 회귀를 찾는 데 사용한다. probe는 small/medium/large 회로,
실제 명령 history, offscreen egui frame, PCB local/full DRC, ratsnest, 저장 DTO와 원자 쓰기를 함께
측정한다. 절대 시간 비교를 기록할 때는 같은 기기, 전원/열 상태와 sample 수를 남긴다.

UI 변경은 가능하면 다음 상태를 직접 확인한다.

- 빈 회로와 좁은 창
- 배치, 배선, 이동, 회전, 삭제 후 undo/redo
- 잘못된 값 입력과 ERC 선택 이동
- 저장/불러오기 왕복 및 recovery status
- Schematic/Breadboard/PCB 작업공간 전환
- DRC 오류가 있는 제작 내보내기 차단

ngspice는 선택 사항이다. 설치되지 않은 환경에서도 전체 내장 기능과 테스트가 동작해야 한다.
