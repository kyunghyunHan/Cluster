# AGENTS.md

이 파일은 Cluster 회로도 편집기 저장소에서 작업하는 Codex, Claude, Cursor, Aider 등 코딩 에이전트를 위한 루트 지침이다.
사람용 설명은 `README.md`를 보고, 에이전트는 작업 전 이 파일을 먼저 기준으로 삼는다.

## Project Overview

Cluster는 Rust + egui 기반 ESP32/Arduino 학습 및 프로토타이핑 회로 툴이다. 목표는 KiCad를 완전히 복제하는 것이 아니라 `Fritzing/Tinkercad처럼 직관적이면서 PCB 제작까지 이어질 수 있는 beginner-friendly KiCad-lite`다.

현재 방향:
- 저항, 커패시터, 인덕터, 다이오드, LED, 스위치, 전원, 배터리, OP AMP, 램프 지원
- ESP32, OLED 같은 실제 전자 모듈 배치 지원
- ESP32/OLED/Sensor는 핀 이름과 역할(VIN/3V3/GND/SDA/SCL/GPIO 등)을 가진다.
- Breadboard View는 회로도 netlist를 읽어 ESP32/Arduino I2C 예제의 점퍼 배선을 안내한다.
- CAD 데이터 모델은 schematic symbol과 physical footprint를 분리하고, netlist를 PCB layout/DRC/export와 공유하는 방향으로 확장한다.
- 스냅 그리드, 90도 배선, 핀 표시, 선택/회전/삭제 지원
- 기본 라이브 시뮬레이션: 닫힌 도통 경로가 있으면 배선과 부품을 강조
- SVG 이미지 내보내기 지원

좋은 변경의 기준:
- 회로를 더 빨리 만들 수 있다.
- 초보자가 실제 브레드보드 배선을 더 빨리 이해할 수 있다.
- schematic에서 PCB 제작 데이터로 넘어가는 흐름이 명확하다.
- 부품과 핀이 더 명확하게 읽힌다.
- 연결됨/끊김/전류 흐름 상태가 즉시 보인다.
- 저장/내보내기 결과가 실제 문서에 쓸 수 있을 만큼 깔끔하다.
- UI가 조밀하지만 피로하지 않다.

## Commercial Product Standard

상용화를 전제로 작업할 때는 "데모로 보이는 기능"보다 "반복 사용해도 신뢰되는 워크플로우"를 우선한다.

제품 기준:
- 사용자가 만든 회로 데이터는 절대 조용히 유실되면 안 된다.
- 저장, 불러오기, 내보내기, 삭제 같은 작업은 성공/실패 상태가 UI에 명확히 남아야 한다.
- 잘못된 회로는 앱이 멈추는 대신 경고와 수정 가능한 상태로 남아야 한다.
- 빈 회로, 큰 회로, 잘못 연결된 회로, 일부 값이 비어 있는 회로를 모두 정상 상태로 취급한다.
- 기능 추가 시 새 사용자가 1분 안에 기본 회로를 만들 수 있는 흐름을 해치지 않는다.
- 전문가가 반복 작업할 때 클릭 수와 마우스 이동이 불필요하게 늘어나지 않아야 한다.

상용화 우선순위:
1. 데이터 안정성: 저장/복원, 자동 복구, 포맷 호환성
2. 회로 검증: short/open/reversed polarity/missing ground 같은 실수 감지
3. 작업 속도: 빠른 배치, 복제, 정렬, 핀 스냅, 키보드 조작
4. 출력 품질: 문서/수업/공유에 바로 쓸 수 있는 SVG/PNG
5. 신뢰성: panic 없는 UI, 명확한 에러 메시지, 느려지지 않는 큰 회로

## Setup Commands

- Build/check: `cargo check`
- Run app: `cargo run`
- Search code: `rg "<pattern>"`
- List files: `rg --files`

Notes:
- 작은 변경에서 전체 구조를 크게 갈아엎지 않는다.
- `src/main.rs` 단일 파일 구조가 크면, 먼저 명확한 함수 단위로 정리하고 이후 모듈 분리를 검토한다.
- 네트워크가 필요한 dependency 추가는 신중히 한다. 가능하면 egui와 표준 라이브러리 안에서 해결한다.

## Testing Instructions

- 코드 변경 후 기본 검증은 `cargo check`다.
- UI 변경은 가능하면 `cargo run`으로 실제 배치, 좁은 패널, 빈 회로 상태를 확인한다.
- 저장/불러오기 변경은 최소한 빈 회로, 단일 부품, 배선 포함 회로, 모듈 포함 회로를 왕복 확인한다.
- 시뮬레이션 변경은 최소한 아래 케이스를 생각한다.
  - 전원 + 부하 + GND가 닫힌 경우 흐름 표시
  - 스위치 `open`/`off`일 때 끊김 표시
  - 전원만 있거나 부하만 있을 때 Open circuit
  - ESP32/OLED 모듈 핀이 live loop에 물렸을 때 활성 표시
- 내보내기 변경은 생성 파일이 브라우저에서 열리는지 확인한다.
- 회귀 위험이 큰 로직은 작고 직접적인 단위 테스트를 추가한다.

## Code Style

- 기존 Rust/egui 스타일을 우선 따른다.
- 큰 리팩터링보다 작고 검증 가능한 개선을 선호한다.
- 불필요한 abstraction을 만들지 않는다.
- UI 텍스트는 짧고 기능 중심으로 쓴다.
- 사용자 변경사항을 되돌리지 않는다.
- manual edit은 좁게 유지한다.
- 코드 식별자는 영어를 사용한다. 문서와 짧은 설명은 한국어를 사용해도 된다.
- 저장 포맷, 시뮬레이션 결과, 에러 상태처럼 장기 유지될 데이터는 명시적인 타입으로 표현한다.
- 사용자가 볼 수 있는 에러 메시지는 짧고 원인/다음 행동을 알 수 있게 쓴다.

## Data And Compatibility

- 회로 저장 포맷은 가능하면 사람이 읽을 수 있는 JSON을 기본으로 한다.
- 저장 데이터에는 앱 버전 또는 schema version을 포함한다.
- 새 필드를 추가할 때는 기존 파일을 읽을 수 있는 기본값을 둔다.
- 불러오기 실패 시 앱이 crash/panic하지 않고 status에 원인을 표시한다.
- 좌표, 회전, 부품 타입, 값, 핀 연결, 스위치 상태, 모듈 핀 역할은 저장/복원 대상이다.
- 포맷 변경은 migration 또는 backwards-compatible parser를 먼저 고려한다.

## Reliability And Error Handling

- UI 이벤트, 파일 I/O, export, parsing 경로에서 `unwrap()`/`expect()`는 피한다. 불변 조건이 코드상 명확한 경우만 예외로 한다.
- 실패 가능한 작업은 `Result`로 다루고, 사용자에게 필요한 메시지를 status 또는 panel에 남긴다.
- 한 부품 또는 한 배선의 오류가 전체 회로 렌더링을 막으면 안 된다.
- 삭제, 덮어쓰기, 초기화 같은 작업은 사용자의 의도를 확인하거나 되돌릴 수 있는 흐름을 둔다.
- 장시간 작업이 생기면 UI가 멈춘 것처럼 보이지 않게 상태를 표시한다.

## Performance Standard

- 일반 작업은 마우스 입력에 즉각 반응해야 한다.
- 큰 회로에서도 pan/zoom/select/wire drawing이 눈에 띄게 버벅이면 안 된다.
- 매 프레임 전체 회로를 불필요하게 재계산하지 않는다. 연결성, bounds, export data는 변경 시점 캐싱을 우선 검토한다.
- 렌더링보다 시뮬레이션/검증 로직이 커질 경우, 계산 단위를 작게 나눠 테스트 가능하게 유지한다.
- 성능 개선은 가독성을 크게 해치지 않는 범위에서 한다.

## Design Standard

기본 방향은 `Dense Practical Lab`이다.

항상 확인할 점:
- 장식보다 회로 가독성이 우선이다.
- 부품명, 값, 핀, 배선이 겹치지 않아야 한다.
- 색상은 의미가 있을 때만 사용한다.
  - 파랑: 일반 배선
  - 주황: live/전류 흐름
  - 초록: 선택
  - 노랑: 핀
- 버튼과 패널은 조밀하게 유지한다.
- 큰 카드, 과한 glow, 불필요한 여백을 피한다.
- OLED/ESP32 같은 모듈은 실제 핀 배열을 떠올릴 수 있게 그린다.
- 좁은 창에서도 주요 도구, 상태, 캔버스가 서로 밀려 사라지지 않아야 한다.
- 색상만으로 상태를 구분하지 않는다. live/open/error/selected는 라벨, 선 스타일, 아이콘, 두께 중 하나를 함께 고려한다.
- 툴팁과 status text는 짧게 쓰고, 작업 흐름을 가리지 않는다.

## Simulation Standard

현재 시뮬레이션은 SPICE급 아날로그 해석이 아니라 연결성 기반 live-path 판정이다.

현재 지원:
- 전원/배터리/전류원과 return/GND 사이에 닫힌 도통 경로가 있으면 `Current flowing`
- 저항/인덕터/다이오드/LED/램프/닫힌 스위치는 도통 부품으로 처리
- 커패시터/OP AMP는 기본 DC 도통 경로에서는 open으로 처리
- ESP32/OLED/Sensor 모듈은 live loop에 핀이 연결되면 활성 표시
- 배선 클릭 위치가 핀 근처이면 자동으로 핀에 스냅한다.
- 부품 핀이 배선 선분 중간에 닿아도 같은 net으로 병합한다.
- DC MNA 분기 전류에서 배선 전류 크기와 방향을 계산해 표시한다.
- 다이오드/LED는 역바이어스에서 누설 수준으로 처리하고 순방향 전압 강하를 적용한다.
- NMOS/PMOS는 Vgs/Vsg 임계전압에 따라 ON/OFF 저항을 전환한다.
- floating pin/net, GND DC 경로가 없는 voltage island, open voltage source, 아무 핀에도 연결되지 않은 배선, 한 핀에만 연결된 고립 배선을 ERC로 경고한다.
- 부품별 simulation support, voltage/current 제한, driver/current-limit 요구사항을 metadata로 관리한다.
- DC solver는 구조화된 오류와 KCL residual, dissipating/supplying power role을 제공한다.
- 시뮬레이션 결과는 OK/Warning/Failed 상태와 초보자용 설명 문장을 포함한다.
- inspector는 부품 model/support, 핀별 net/전압, wire net/current/open 상태를 표시한다.
- ESP/Pico GPIO 5V, GPIO 직접 부하, relay flyback diode, I2C pull-up 누락을 ERC로 경고한다.
- 한 polyline 안에서 분기 전류가 달라지면 잘못된 단일 전류 화살표를 표시하지 않는다.
- 정상 ESP32/Arduino I2C 예제는 4.7k SDA/SCL pull-up을 포함한다.

향후 시뮬레이션 방향:
- 핀 역할(VCC/GND/SDA/SCL/GPIO)을 구조화한다.
- LED 방향성, 다이오드 방향성, 스위치 상태를 UI로 명확히 바꾼다.
- 저항값 기반 전류 추정, 전압 강하, 쇼트 경고를 추가한다.
- 완전한 SPICE 연동은 별도 solver 도입 전까지 범위 밖으로 둔다.

검증 메시지 방향:
- `Open circuit`: 전원과 return/GND 사이 경로가 없음
- `Short risk`: 전원과 GND가 부하 없이 직접 연결될 가능성
- `Polarity warning`: LED/다이오드/전해 커패시터 방향 의심
- `Signal mismatch`: SDA/SCL/UART/SPI 핀이 예상 역할과 다르게 연결됨
- 경고는 작업을 막기보다, 문제가 있는 부품/핀을 찾기 쉽게 표시한다.

## Export Standard

- 이미지 저장은 기본적으로 SVG를 우선한다.
- SVG는 배선, 부품 박스, 라벨, 값, 핀을 포함해야 한다.
- 파일명은 충돌이 적고 사용자가 바로 찾을 수 있어야 한다.
- 내보내기 실패 시 status에 에러를 표시한다.
- export 결과는 배경이 투명/흰색일 때 모두 읽을 수 있어야 한다.
- 문서 삽입을 위해 적절한 viewBox, 여백, 텍스트 크기를 유지한다.
- PNG export를 추가할 때도 SVG export 품질을 떨어뜨리지 않는다.
- SPICE `.cir`, BOM CSV, Arduino starter sketch export는 회귀 테스트로 검증한다.
- README 예제 이미지는 앱의 SVG exporter로 재생성할 수 있어야 한다.
- CI는 `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, release build를 실행한다.

## Release Readiness

릴리즈를 목표로 하는 변경은 아래를 고려한다.
- 앱 시작, 새 회로, 저장, 불러오기, 내보내기, 기본 부품 배치가 모두 동작한다.
- panic이 발생할 수 있는 사용자 입력 경로를 점검한다.
- 기본 예제 회로 또는 샘플 파일이 최신 저장 포맷으로 열리는지 확인한다.
- 릴리즈 노트에 사용자에게 보이는 변경, breaking change, 알려진 제한을 남길 수 있게 변경 내용을 정리한다.
- 새 dependency는 라이선스, 유지보수 상태, binary size, offline build 영향을 검토한 뒤 추가한다.
- `v*` 태그 릴리스는 Linux/macOS/Windows 바이너리 압축 파일을 생성한다.

## Roadmap

우선순위 높은 순서:
1. Breadboard View: 일부 완료 - ESP32/Arduino/STM32 + OLED/Sensor I2C 예제의 VCC/GND/SDA/SCL 점퍼 체크, schematic net 강조, 누락 점퍼 자동 schematic 배선 추가 지원. 향후 실제 점퍼 편집, 전원 레일, 핀 하이라이트 확장
2. CAD/PCB 데이터 모델: 일부 완료 - SymbolInstance, Footprint, NetClass, Board, Track, Via, 기본 DRC, Gerber/Excellon scaffold, bottom dock Update PCB/footprint auto-place/PCB preview+DRC marker/board fit/ratsnest route helper/selectable DRC rows/project folder save-load/DRC-gated fabrication export 추가. 향후 기존 Component를 SymbolInstance로 점진 이전
3. Schematic netlist 안정화: 일부 완료 - deterministic net name/id 생성, global GND merge, explicit junction/no-connect annotation 모델, crossing/T-junction/pin-to-wire/multi-page label 회귀 테스트 추가. 향후 UI 저장/편집, local/global label scope 확장
4. 초보자 ERC 강화: 일부 완료 - GPIO 전류 초과, LED 저항 누락, 모터/릴레이 직접 구동, 공통 GND, 입력 전용 GPIO, ADC 과전압, I2C/SPI/UART 배선 실수, ERC repair suggestion/Auto fix UI scaffold, I2C pull-up/relay flyback diode 자동 배선 repair 추가. 향후 더 많은 자동 배선/정확한 위치 삽입 확장
5. PCB editor MVP: 일부 완료 - bottom dock Update PCB, footprint auto-place, compact PCB preview, preview DRC marker, board fit, unrouted ratsnest route helper, fabrication export, footprint/ratsnest/selectable DRC 요약 지원. 향후 interactive footprint 배치, manual routing 편집, via, top/bottom copper, board outline
6. DRC panel: 일부 완료 - track width/clearance/via/annular ring/edge clearance/open outline/outside footprint/unrouted ratsnest 검사 scaffold, PCB dock DRC row 선택 및 preview marker 추가. 향후 pad/silkscreen violation 클릭 이동 UI 연결
7. Export: 일부 완료 - SVG/SPICE/BOM/Arduino 유지, Gerber RS-274X/Excellon scaffold, BOM/CPL CSV helper, PCB DRC error가 있으면 fabrication export 차단, 선택적 ngspice batch 실행 경로 추가. 향후 UI export wizard와 ngspice 결과 plot 연결
8. 실제 부품 라이브러리: ESP32 DevKit V1, Arduino Uno, Pico, STM32 Blue Pill/Nucleo, SSD1306 OLED, DHT11/DHT22, PIR, DS3231, Relay Module, L298N, SG90, buzzer 등 한국어/영어 검색 지원
9. 프로젝트 관리: 일부 완료 - PCB dock에서 `project.cluster/` 폴더에 schematic/board/project JSON 저장 및 로드 지원. 향후 `.cluster` 파일, 자동 저장, 복구, 최근 프로젝트, 프로젝트 썸네일
10. 고급 시뮬레이션: internal beginner DC 유지 + 선택적 ngspice export/run/import

## Definition Of Done

작업 완료 전 확인한다.
- [ ] 기능이 실제 UI에서 접근 가능하다.
- [ ] 빈 회로 상태에서도 깨지지 않는다.
- [ ] 좁은 UI에서도 읽을 수 있다.
- [ ] 사용자 데이터가 유실될 수 있는 경로를 고려했다.
- [ ] 실패 가능한 작업은 status/error로 확인 가능하다.
- [ ] 시뮬레이션 변경이면 open/closed 케이스를 고려했다.
- [ ] 저장/불러오기 변경이면 기존 파일 호환성과 왕복 저장을 확인했다.
- [ ] 내보내기 변경이면 파일이 생성된다.
- [ ] 큰 회로에서 불필요한 매 프레임 재계산을 만들지 않았다.
- [ ] 기존 사용자 변경을 되돌리지 않았다.
- [ ] `cargo check`가 통과한다.
- [ ] 완료/할일 상태가 바뀌면 `AGENTS.md`를 업데이트한다.
