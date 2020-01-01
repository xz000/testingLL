using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillD : MonoBehaviour
{
    public Toggle D1;
    public Toggle D2;
    public Toggle D3;
    public Toggle D4;
    //public Toggle D5;
    public Image IconD;
    public SkillsLink sls;
    GameObject Soldier;

    public void SetD()
    {
        sls.KeyDSkill = null;
        if (D1.isOn)
        {
            sls.KeyDSkill = SkillCode.TestSkillLightning;
            return;
        }
        if (D2.isOn)
        {
            sls.KeyDSkill = SkillCode.SkillD2;
            return;
        }
        if (D3.isOn)
        {
            sls.KeyDSkill = SkillCode.SkillD3;
            return;
        }
        if (D4.isOn)
        {
            sls.KeyDSkill = SkillCode.SkillD4;
            return;
        }
    }
}
