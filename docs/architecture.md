# Cluster architecture

이 문서는 현재 구현의 진입점과 소유권 경계를 설명한다. 장기 제안이 아니라 소스에서 실제로
사용하는 구조를 기준으로 한다.

## 문서와 편집 상태

`ProjectDocument`는 저장되는 회로도 페이지, 부품, 배선, 주석, PCB 보드와 ID 할당 상태를
소유한다. `EditorState`는 선택, 활성 도구, 드래그, 배선 미리보기와 명령 이력을 소유하고,
`WorkspaceState`는 패널과 작업공간 같은 표시 상태를 소유한다. 분석 결과와 파생 인덱스는
`AnalysisState`에 있으며 저장 파일에 직렬화하지 않는다.

사용자 편집은 `EditorCommand`와 제한된 `CommandContext`를 통과한다. 명령 결과의 `ChangeSet`이
연결성, 전기 매개변수, PCB, 저장 dirty 상태를 표시한다. 일반 편집은 영향받은 엔티티만 담는
`DocumentDelta`를 history에 추가하며, 불러오기처럼 문서 전체가 바뀌는 경계만 전체 스냅샷을
사용한다. 한 번의 드래그와 한 번의 ERC 자동 수정은 각각 history 항목 하나다.

## 연결성과 네트리스트

`engine/connectivity/`가 endpoint 해석, 공간 후보 검색, 교차/접합 판정, 라벨 병합,
union-find와 진단 단계를 수행한다. `Pin`, `Junction`, `FreePoint` endpoint는 그려진 polyline과
분리되어 저장된다. 선이 시각적으로 교차한다는 이유만으로 연결하지 않으며, T 접합이나 명시적
junction만 해당 접점을 합친다.

`engine/netlist.rs`는 이 결과에서 결정적인 net ID와 이름, pin/wire/segment/junction의 정확한
mapping을 만든다. 루트와 엔티티를 정렬하므로 입력 배열 순서가 결과를 바꾸지 않는다. ERC,
MNA, Breadboard, PCB 투영, SPICE와 firmware starter가 같은 canonical projection을 공유한다.

## ERC와 시뮬레이션

ERC는 egui와 분리된 rule registry다. 규칙은 stable rule ID, severity, 설명, 관련 엔티티,
수정 안내와 선택적인 안전한 자동 수정을 반환한다. topology/value/dynamic 의존성을 나누어 값만
바뀐 경우 canonical connectivity를 재사용한다.

내장 시뮬레이터는 교육용 MNA DC/AC와 제한된 RC/PWM transient다. SPICE 정확도를 주장하지
않으며 복잡한 모델은 근사 또는 symbol-only로 표시한다. 외부 ngspice backend는 선택 사항이고
실패와 취소를 UI 렌더링 경로 밖에서 처리한다.

공학 값은 `engine/units.rs`의 단일 parser를 사용한다. 대문자 `M`은 UI의 일반 SI 의미대로
mega이고 소문자 `m`은 milli다. SPICE 출력은 `M` 의미 차이를 피하도록 검증된 값을 과학 표기법으로
정규화한다.

## PCB, DRC와 ECO

`pcb::Board`가 footprint, track, via, zone, outline, layer, net class와 설계 규칙을 소유한다.
풋프린트 pad 조회, 화면 표시, 공간 인덱스, Gerber, Excellon과 CPL은 공통
`FootprintTransform`을 사용한다. 편집 명령은 보드 인덱스를 증분 갱신하고 영향을 받는 track/net에
로컬 DRC를 실행하며, 전체 DRC는 분석 작업자에서 실행한다.

회로도→PCB 동기화는 CAD projection과 ECO report를 만든다. 기존 수동 배치를 보존하고 회로도에서
제거된 배치 부품을 조용히 삭제하지 않고 orphan으로 남긴다. 구조 validator나 blocking DRC 오류가
있으면 fabrication export를 파일 생성 전에 차단한다.

## 저장과 캐시 무효화

회로도 JSON은 schema version을 가지며 이전 endpoint 형식을 load 경계에서 migration한다. 저장은
같은 디렉터리의 임시 파일을 sync한 뒤 rename하며 `.bak` 3세대를 회전한다. load는 복구 가능한
데이터를 보존하고 구조화된 진단을 status에 남긴다.

`DocumentRevisions`는 schematic geometry/connectivity, electrical parameters, PCB와 visual 변경을
분리한다. 분석 작업자는 revision이 일치하는 결과만 게시하고 오래된 결과를 폐기한다. 팬, 줌,
선택과 PCB-only 변경은 회로도 연결성 캐시를 무효화하지 않는다.

더 자세한 측정 근거와 미완성 범위는
`docs/architecture/commercial-completion-audit-2026-07-23.md`를 참고한다.
