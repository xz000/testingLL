using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillE : MonoBehaviour
{
    public Toggle E1;
    public Toggle E1a;
    public Toggle E1b;
    public Toggle E2;
    public Toggle E2a;
    public Toggle E2b;
    public Toggle E3;
    public Toggle E3a;
    public Toggle E3b;
    public Toggle E4;
    public Toggle E4a;
    public Toggle E4b;
    public Image IconE;
    GameObject Soldier;

    public void SetE()
    {
        GetComponent<SkillsLink>().KeyESkill = null;
        if (E1.isOn && E1a.isOn)
        {
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillE1;
            return;
        }
        if (E1.isOn && E1b.isOn)
        {
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillE1b;
            return;
        }
        if (E2.isOn && E2a.isOn)
        {
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillE2;
            return;
        }
        if (E2.isOn && E2b.isOn)
        {
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillE2b;
            return;
        }
        if (E3.isOn && E3a.isOn)
        {
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillE3;
            return;
        }
        if (E3.isOn && E3b.isOn)
        {
            GetComponent<SkillsLink>().KeyESkill = SkillCode.SkillE3b;
            return;
        }
    }
}
