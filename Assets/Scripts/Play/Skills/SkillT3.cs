using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillT3 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    //public float maxdistance;
    public float bulletspeed = 20;
    public GameObject fireball;
    public float damage = 5;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillT3()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
        }
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        GetComponent<DoSkill>().BeforeSkill();
        Fix64Vector2 singplace = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 skilldirection = (actionplace - singplace).normalized();
        DoFire((singplace + (Fix64)0.81 * skilldirection).ToV2(), (skilldirection * (Fix64)bulletspeed).ToV2());
        currentcooldown = 0;
        skillavaliable = false;
    }

    void DoFire(Vector2 fireplace, Vector2 speed2d)
    {
        GameObject bullet;
        fireball.GetComponent<JumpBulletScript>().sender = gameObject;
        bullet = Instantiate(fireball, fireplace, Quaternion.identity);
        bullet.GetComponent<JumpBulletScript>().Damage = damage;
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d;
    }

    void SkillT3SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }

    void LinkToIcon()
    {
        if (enabled)
            GameObject.Find("Canvas2").GetComponent<BottomLink>().iT.Fif = CalcFA;
    }
}
