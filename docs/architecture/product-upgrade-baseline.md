# 제품 업그레이드 기준선

기준일: 2026-07-14 (호주/멜버른).

작업 트리에는 앞서 진행한 정규 연결성 및 명령 경계 리팩터링이 이미 포함되어 있었다.
해당 변경을 보존하고 이번 업그레이드의 출발점으로 삼는다.

## 업그레이드 전 소스 소유권

| 관심사 | 현재 소유자 | 경계 문제 |
| --- | --- | --- |
| 영속 문서 | `ui::app::CircuitApp`의 평면 필드, `model::circuit`의 DTO, `PcbUiState`의 PCB 보드 | 영속 데이터와 런타임/UI 상태가 하나의 애플리케이션 객체를 공유한다. |
| 편집기 상태 | `CircuitApp`, `app::state`, `HistoryState`, `editor::history` | 도구, 선택, 클립보드, 드래그, 이력이 하나의 소유 경계로 묶여 있지 않다. |
| 작업공간 UI | `UiState`, `CanvasState`, 팔레트/인스펙터/시뮬레이션/브레드보드/PCB UI 구조체 | 보기 전용 필드 일부가 여전히 `CircuitApp`에 평면적으로 남아 있다. |
| 연결성 | `engine::netlist`에서 구성하는 `model::graph::CanonicalConnectivity` | 정규 결과는 있지만 1,300줄 이상의 빌더가 모든 단계와 테스트를 여전히 섞고 있다. |
| ERC | `engine::validation`과 `ui::app::energize`의 UI 지향 ERC 코드 | 2,700줄 이상의 단일체이며 규칙이 등록된 검사가 아닌 함수 형태다. |
| 시뮬레이션 | `engine::simulation`의 퍼사드, MNA 모듈과 `ui::app::energize`의 구현 | 엔진 소유권이 UI 코드로 역전되어 있다. |
| PCB | `pcb::*`, `PcbUiState`, `app::actions`의 오케스트레이션 | 보드 영속성/편집 상태와 미리보기 UI가 결합되어 있다. |
| 영속성 | `storage::save`, `storage::autosave`, `app::actions`의 중복 변환/불러오기 오케스트레이션 | 백업 쓰기가 아직 완전히 동기화된 원자적 교체/복구 시스템이 아니다. |
| 실행 취소/다시 실행 | `HistoryState`와 `editor::history`의 스냅샷 스택, `commands`의 명령 디스패처 | 명령이 변경 상태를 보고하지만 되돌릴 수 있는 델타를 소유하거나 완료된 드래그를 병합하지 않는다. |
| 캐시 무효화 | `CommandDirtyState`, `DirtyFlags`, `editor::history`, `app::actions`의 캐시 접근자 | 수동 무효화 경로가 디스패처 무효화와 공존한다. |

## 확인한 직렬화 스키마

- 회로도 JSON: 스키마 4 (`SavedCircuit`, `SavedPage`, 타입 지정 선택적 배선 끝점).
- CAD 프로젝트: 스키마 1.
- 보드: 스키마 1.
- 사용자 부품: 스키마 1, 지원하지 않는 미래 버전은 거부한다.
- 라이브러리 카탈로그: 스키마 1.

런타임 아키텍처 타입 도입만으로 스키마를 변경하지 않는다. 호환성 파싱과 레거시 끝점
마이그레이션은 계속 영속성 경계에 둔다.

## 필수 명령 기준선

| 명령 | 결과 |
| --- | --- |
| `cargo fmt --check` | 통과. |
| `cargo clippy --all-targets --all-features -- -D warnings` | 실패: `engine/netlist.rs`의 정규 연결성 테스트 시그니처에서 `clippy::type_complexity` 1건. 앞선 리팩터링에서 발생했으며, 이 기준선을 기록한 직후 테스트 전용 명명 타입 별칭으로 수정한다. |
| `cargo test --all-targets` | 통과: 195개 테스트. |
| `cargo build --release` | 통과. |

clippy 실패는 억제하거나 누락하지 않고 그대로 기록한다.
