using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillR : MonoBehaviour
{
    public Toggle R1;
    public Toggle R1a;
    public Toggle R1b;
    public Toggle R2;
    public Toggle R2a;
    public Toggle R2b;
    public Toggle R3;
    public Toggle R3a;
    public Toggle R3b;
    public Image IconR;

    public void SetR()
    {
        GetComponent<SkillsLink>().KeyRSkill = null;
        if (R1.isOn && R1a.isOn)
        {
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR1;
            return;
        }
        if (R1.isOn && R1b.isOn)
        {
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR1b;
            return;
        }
        if (R2.isOn && R2a.isOn)
        {
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR2;
            return;
        }
        if (R2.isOn && R2b.isOn)
        {
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR2b;
            return;
        }
        if (R3.isOn && R3a.isOn)
        {
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.TestSkill02;
            return;
        }
        if (R3.isOn && R3b.isOn)
        {
            GetComponent<SkillsLink>().KeyRSkill = SkillCode.SkillR3b;
            return;
        }
    }
}
